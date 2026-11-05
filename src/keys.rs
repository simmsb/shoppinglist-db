use gluesql::core::data::Key;

const NAMESPACE: &[u8] = b"gluesql/v1/";

const SCHEMA: u8 = 1;
const DATA: u8 = 2;

fn prefixed(bytes: &[u8]) -> Vec<u8> {
    let len = u32::try_from(bytes.len()).unwrap();
    let mut out = Vec::with_capacity(4 + bytes.len());
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(bytes);
    out
}

fn namespaced(kind: u8) -> Vec<u8> {
    let mut key = Vec::with_capacity(NAMESPACE.len() + 1);
    key.extend_from_slice(NAMESPACE);
    key.push(kind);
    key
}

pub(crate) fn schema_key(table_name: &str) -> Vec<u8> {
    let mut key = namespaced(SCHEMA);
    key.extend(prefixed(table_name.as_bytes()));
    key
}

pub(crate) fn data_prefix(table_name: &str) -> Vec<u8> {
    let mut key = namespaced(DATA);
    key.extend(prefixed(table_name.as_bytes()));
    key
}

pub(crate) fn data_key(table_name: &str, key: &Key) -> Vec<u8> {
    let encoded = crate::serialize(key).expect("Failed to serialize key, shouldn't happen");
    let mut out = data_prefix(table_name);
    out.extend(prefixed(&encoded));
    out
}

pub(crate) fn schema_prefix() -> Vec<u8> {
    namespaced(SCHEMA)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_prefix_free(left: &[u8], right: &[u8]) {
        assert_ne!(left, right);
        assert!(!left.starts_with(right), "left starts with right");
        assert!(!right.starts_with(left), "right starts with left");
    }

    #[test]
    fn table_names_and_row_keys_are_prefix_free() {
        let item = data_prefix("item");
        let items = data_prefix("items");
        assert_prefix_free(&item, &items);

        let short = data_key("item", &Key::Str("a".to_owned()));
        let long = data_key("item", &Key::Str("ab".to_owned()));
        assert!(short.starts_with(&item));
        assert!(!long.starts_with(&short));
        assert!(!data_key("items", &Key::Str("a".to_owned())).starts_with(&item));

        let bytes_short = data_key("t", &Key::Bytea(vec![1, 2]));
        let bytes_long = data_key("t", &Key::Bytea(vec![1, 2, 3]));
        assert_prefix_free(&bytes_short, &bytes_long);

        assert_prefix_free(&schema_key("item"), &schema_key("items"));
        assert_prefix_free(&schema_key("item"), &short);
        assert_prefix_free(
            &data_key("item", &Key::Uuid(1)),
            &data_key("item", &Key::Uuid(2)),
        );
    }
}
