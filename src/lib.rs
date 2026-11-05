mod keys;

use std::pin::pin;

use gluesql::core::{
    data::{CustomFunction as Function, Key, Schema, Value},
    error::{Error, Result},
    store::{
        AlterTable, CustomFunction, CustomFunctionMut, Index, IndexMut, Metadata, Planner, RowIter,
        Store, StoreMut, Transaction,
    },
};
use iroh_docs::{AuthorId, api::Doc, store::Query};
use n0_future::{StreamExt, future::block_on};
use uuid::Uuid;

use crate::keys::{data_key, data_prefix, schema_key, schema_prefix};

#[derive(Debug, Clone)]
pub struct GluesqlDb {
    doc: Doc,
    blobs: iroh_blobs::api::Store,
    author: AuthorId,
}

impl GluesqlDb {
    // Open a gluesql database on top of an Iroh document.
    pub fn new(doc: Doc, blobs: iroh_blobs::api::Store, author: AuthorId) -> Self {
        Self { doc, blobs, author }
    }

    pub fn doc(&self) -> &Doc {
        &self.doc
    }

    pub fn blobs(&self) -> &iroh_blobs::api::Store {
        &self.blobs
    }

    pub fn author(&self) -> AuthorId {
        self.author
    }

    async fn entries(&self, query: Query) -> Result<Vec<iroh_docs::Entry>> {
        let stream = self.doc.get_many(query).await.map_err(storage_err)?;
        let mut stream = pin!(stream);
        let mut entries = Vec::new();
        while let Some(entry) = stream.next().await {
            entries.push(entry.map_err(storage_err)?);
        }
        Ok(entries)
    }

    /// Reads the latest entry for the given key.
    async fn read_bytes(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let query = Query::single_latest_per_key().key_exact(key).build();
        let Some(entry) = self.doc.get_one(query).await.map_err(storage_err)? else {
            return Ok(None);
        };
        if entry.content_len() == 0 {
            return Ok(None);
        }
        let bytes = self
            .blobs
            .get_bytes(entry.content_hash())
            .await
            .map_err(storage_err)?;
        Ok(Some(bytes.to_vec()))
    }

    async fn write_bytes(&self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        self.doc
            .set_bytes(self.author, key, value)
            .await
            .map(|_| ())
            .map_err(storage_err)
    }

    async fn delete_key(&self, key: Vec<u8>) -> Result<()> {
        self.doc
            .del(self.author, key)
            .await
            .map(|_| ())
            .map_err(storage_err)
    }

    async fn fetch_schema_async(&self, table_name: &str) -> Result<Option<Schema>> {
        let key = schema_key(table_name);
        let Some(bytes) = self.read_bytes(&key).await? else {
            return Ok(None);
        };
        let schema: Schema = deserialize(&bytes).map_err(storage_err)?;
        Ok(Some(schema))
    }

    async fn fetch_all_schemas_async(&self) -> Result<Vec<Schema>> {
        let query = Query::single_latest_per_key()
            .key_prefix(schema_prefix())
            .build();
        let mut schemas = Vec::new();
        for entry in self.entries(query).await? {
            if entry.content_len() == 0 {
                continue;
            }
            let bytes = self
                .blobs
                .get_bytes(entry.content_hash())
                .await
                .map_err(storage_err)?;
            let schema: Schema = deserialize(&bytes).map_err(storage_err)?;
            schemas.push(schema);
        }
        schemas.sort_by(|left, right| left.table_name.cmp(&right.table_name));
        Ok(schemas)
    }

    async fn scan_rows(&self, table_name: &str) -> Result<Vec<(Key, Vec<Value>)>> {
        let prefix = data_prefix(table_name);
        let query = Query::single_latest_per_key().key_prefix(&prefix).build();
        let mut rows = Vec::new();
        for entry in self.entries(query).await? {
            if entry.content_len() == 0 || entry.key().len() == prefix.len() {
                continue;
            }

            // TODO: maybe do this with bounded concurrency?
            let bytes = self
                .blobs
                .get_bytes(entry.content_hash())
                .await
                .map_err(storage_err)?;
            let row: (Key, Vec<Value>) = deserialize(&bytes).map_err(storage_err)?;
            rows.push(row);
        }
        Ok(rows)
    }

    async fn put_row(&self, table_name: &str, key: &Key, row: &[Value]) -> Result<()> {
        let doc_key = data_key(table_name, key);
        let bytes = serialize(&(key, row)).map_err(storage_err)?;
        self.write_bytes(doc_key, bytes).await
    }
}

impl Store for GluesqlDb {
    fn fetch_schema(&self, table_name: &str) -> Result<Option<Schema>> {
        block_on(self.fetch_schema_async(table_name))
    }

    fn fetch_all_schemas(&self) -> Result<Vec<Schema>> {
        block_on(self.fetch_all_schemas_async())
    }

    fn fetch_data(&self, table_name: &str, key: &Key) -> Result<Option<Vec<Value>>> {
        block_on(async {
            let doc_key = data_key(table_name, key);
            let Some(bytes) = self.read_bytes(&doc_key).await? else {
                return Ok(None);
            };
            let (_key, row): (Key, Vec<Value>) = deserialize(&bytes).map_err(storage_err)?;
            Ok(Some(row))
        })
    }

    fn scan_data<'a>(&'a self, table_name: &str) -> Result<RowIter<'a>> {
        let rows = block_on(self.scan_rows(table_name))?;
        Ok(Box::new(rows.into_iter().map(Ok)))
    }
}

impl StoreMut for GluesqlDb {
    fn insert_schema(&mut self, schema: &Schema) -> Result<()> {
        block_on(async {
            let key = schema_key(&schema.table_name);
            let bytes = serialize(schema).map_err(storage_err)?;
            self.write_bytes(key, bytes).await
        })
    }

    fn delete_schema(&mut self, table_name: &str) -> Result<()> {
        block_on(async {
            let rows = self.scan_rows(table_name).await?;
            for (key, _) in rows {
                let doc_key = data_key(table_name, &key);
                self.delete_key(doc_key).await?;
            }
            let schema = schema_key(table_name);
            self.delete_key(schema).await
        })
    }

    fn append_data(&mut self, table_name: &str, rows: Vec<Vec<Value>>) -> Result<()> {
        block_on(async {
            if rows.is_empty() || self.fetch_schema_async(table_name).await?.is_none() {
                return Ok(());
            }

            for row in rows {
                let key = Key::Uuid(Uuid::new_v4().as_u128());
                self.put_row(table_name, &key, &row).await?;
            }
            Ok(())
        })
    }

    fn insert_data(&mut self, table_name: &str, rows: Vec<(Key, Vec<Value>)>) -> Result<()> {
        block_on(async {
            if self.fetch_schema_async(table_name).await?.is_none() {
                return Ok(());
            }
            for (key, row) in rows {
                self.put_row(table_name, &key, &row).await?;
            }
            Ok(())
        })
    }

    fn delete_data(&mut self, table_name: &str, keys: Vec<Key>) -> Result<()> {
        block_on(async {
            if self.fetch_schema_async(table_name).await?.is_none() {
                return Ok(());
            }
            for key in keys {
                let doc_key = data_key(table_name, &key);
                self.delete_key(doc_key).await?;
            }
            Ok(())
        })
    }
}

impl AlterTable for GluesqlDb {}
impl Index for GluesqlDb {}
impl IndexMut for GluesqlDb {}
impl Metadata for GluesqlDb {}
impl Transaction for GluesqlDb {}
impl Planner for GluesqlDb {}
impl CustomFunctionMut for GluesqlDb {}

impl CustomFunction for GluesqlDb {
    fn fetch_function(&self, _func_name: &str) -> Result<Option<&Function>> {
        Ok(None)
    }

    fn fetch_all_functions(&self) -> Result<Vec<&Function>> {
        Ok(Vec::new())
    }
}

fn storage_err(error: impl ToString) -> Error {
    Error::StorageMsg(error.to_string())
}

pub(crate) fn serialize<E: serde::ser::Serialize>(val: E) -> Result<Vec<u8>, bincode::error::EncodeError> {
    bincode::encode_to_vec(bincode::serde::Compat(val), bincode::config::standard())
}

pub(crate) fn deserialize<D: serde::de::DeserializeOwned>(src: &[u8]) -> Result<D, bincode::error::DecodeError> {
    let (bincode::serde::Compat(x), _) = bincode::decode_from_slice(src, bincode::config::standard())?;
    Ok(x)
}

#[cfg(test)]
mod tests {
    use gluesql::core::data::{Key, Value};
    use super::{serialize, deserialize};

    #[test]
    fn row_payload_roundtrips_through_bincode() {
        let key = Key::I64(7);
        let row = vec![Value::I64(7), Value::Str("milk".to_owned())];
        let bytes = serialize(&(&key, row.as_slice())).unwrap();
        let (decoded_key, decoded_row): (Key, Vec<Value>) = deserialize(&bytes).unwrap();
        assert_eq!(decoded_key, key);
        assert_eq!(decoded_row, row);
    }
}
