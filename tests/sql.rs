use gluesql::core::{
    data::Value,
    prelude::{Glue, Payload},
};
use iroh::{Endpoint, endpoint::presets, protocol::Router};
use iroh_blobs::{BlobsProtocol, store::mem::MemStore};
use iroh_docs::protocol::Docs;
use iroh_gossip::net::Gossip;
use shoppinglist_db::GluesqlDb;

struct Node {
    _blobs: MemStore,
    _router: Router,
    docs: Docs,
    db: GluesqlDb,
}

async fn node() -> Node {
    let endpoint = Endpoint::bind(presets::Minimal).await.unwrap();
    let blobs = MemStore::new();
    let blob_store = (*blobs).clone();
    let gossip = Gossip::builder().spawn(endpoint.clone());
    let docs = Docs::memory()
        .spawn(endpoint.clone(), blob_store.clone(), gossip.clone())
        .await
        .unwrap();
    let router = Router::builder(endpoint)
        .accept(iroh_blobs::ALPN, BlobsProtocol::new(&blobs, None))
        .accept(iroh_gossip::ALPN, gossip)
        .accept(iroh_docs::ALPN, docs.clone())
        .spawn();
    let author = docs.author_create().await.unwrap();
    let doc = docs.create().await.unwrap();
    let db = GluesqlDb::new(doc, blob_store, author);
    Node {
        _blobs: blobs,
        _router: router,
        docs,
        db,
    }
}

fn select(glue: &mut Glue<GluesqlDb>, sql: &str) -> Vec<Vec<Value>> {
    match glue.execute(sql).unwrap().pop().unwrap() {
        Payload::Select { rows, .. } => rows,
        other => panic!("expected select, got {other:?}"),
    }
}

fn text(value: &str) -> Value {
    Value::Str(value.to_owned())
}

#[tokio::test(flavor = "multi_thread")]
async fn one_document_is_one_database() {
    let node = node().await;
    let doc_id = node.db.doc().id();

    tokio::task::block_in_place(|| {
        let mut glue = Glue::new(node.db.clone());
        glue.execute(
            "CREATE TABLE item (id INTEGER PRIMARY KEY, name TEXT);
             CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);
             INSERT INTO item VALUES (1, 'one');
             INSERT INTO items VALUES (1, 'many'), (2, 'more');",
        )
        .unwrap();

        assert_eq!(
            select(
                &mut glue,
                "SELECT items.name FROM items JOIN item ON items.id = item.id ORDER BY items.id",
            ),
            vec![vec![Value::Str("many".to_owned())]]
        );

        glue.execute("DELETE FROM item WHERE id = 1").unwrap();
        assert_eq!(
            select(&mut glue, "SELECT name FROM items ORDER BY id"),
            vec![
                vec![Value::Str("many".to_owned())],
                vec![Value::Str("more".to_owned())],
            ]
        );
        assert!(select(&mut glue, "SELECT * FROM item").is_empty());

        glue.execute(
            "CREATE TABLE log (msg TEXT);
             INSERT INTO log VALUES ('a'), ('b');
             UPDATE items SET name = 'several' WHERE id = 2;",
        )
        .unwrap();
        assert_eq!(
            select(&mut glue, "SELECT name FROM items WHERE id = 2"),
            vec![vec![Value::Str("several".to_owned())]]
        );

        glue.execute("ALTER TABLE items ADD COLUMN note TEXT DEFAULT 'x'")
            .unwrap();
        assert_eq!(
            select(&mut glue, "SELECT note FROM items ORDER BY id"),
            vec![
                vec![Value::Str("x".to_owned())],
                vec![Value::Str("x".to_owned())],
            ]
        );

        glue.execute("DROP TABLE log").unwrap();
        glue.execute("CREATE TABLE log (msg TEXT); INSERT INTO log VALUES ('c')")
            .unwrap();
        assert_eq!(
            select(&mut glue, "SELECT msg FROM log"),
            vec![vec![Value::Str("c".to_owned())]]
        );
    });

    let reopened = node.docs.open(doc_id).await.unwrap().unwrap();
    let db = GluesqlDb::new(reopened, node.db.blobs().clone(), node.db.author());
    tokio::task::block_in_place(|| {
        let mut glue = Glue::new(db);
        glue.execute("INSERT INTO log VALUES ('d'), ('e')").unwrap();
        assert_eq!(
            select(&mut glue, "SELECT msg FROM log ORDER BY msg"),
            vec![
                vec![Value::Str("c".to_owned())],
                vec![Value::Str("d".to_owned())],
                vec![Value::Str("e".to_owned())],
            ]
        );
        assert_eq!(
            select(&mut glue, "SELECT name FROM items ORDER BY id"),
            vec![
                vec![Value::Str("many".to_owned())],
                vec![Value::Str("several".to_owned())],
            ]
        );
    });
}

#[tokio::test(flavor = "multi_thread")]
async fn adding_and_dropping_columns() {
    let node = node().await;
    let doc_id = node.db.doc().id();

    tokio::task::block_in_place(|| {
        let mut glue = Glue::new(node.db.clone());
        glue.execute(
            "CREATE TABLE item (
                id INTEGER PRIMARY KEY,
                name TEXT,
                qty INTEGER
             );
             INSERT INTO item VALUES (1, 'milk', 2), (2, 'bread', 1);",
        )
        .unwrap();

        glue.execute("ALTER TABLE item ADD COLUMN note TEXT DEFAULT 'fresh'")
            .unwrap();
        assert_eq!(
            select(
                &mut glue,
                "SELECT id, name, qty, note FROM item ORDER BY id"
            ),
            vec![
                vec![Value::I64(1), text("milk"), Value::I64(2), text("fresh")],
                vec![Value::I64(2), text("bread"), Value::I64(1), text("fresh")],
            ]
        );

        glue.execute("INSERT INTO item (id, name, qty) VALUES (3, 'eggs', 12)")
            .unwrap();
        glue.execute("INSERT INTO item VALUES (4, 'tea', 1, 'loose')")
            .unwrap();
        glue.execute("ALTER TABLE item ADD COLUMN rating INTEGER")
            .unwrap();
        let with_rating = vec![
            vec![
                Value::I64(1),
                text("milk"),
                Value::I64(2),
                text("fresh"),
                Value::Null,
            ],
            vec![
                Value::I64(2),
                text("bread"),
                Value::I64(1),
                text("fresh"),
                Value::Null,
            ],
            vec![
                Value::I64(3),
                text("eggs"),
                Value::I64(12),
                text("fresh"),
                Value::Null,
            ],
            vec![
                Value::I64(4),
                text("tea"),
                Value::I64(1),
                text("loose"),
                Value::Null,
            ],
        ];
        assert_eq!(
            select(
                &mut glue,
                "SELECT id, name, qty, note, rating FROM item ORDER BY id"
            ),
            with_rating
        );

        let duplicate = glue
            .execute("ALTER TABLE item ADD COLUMN name TEXT")
            .unwrap_err()
            .to_string();
        assert!(duplicate.contains("Already existing column"), "{duplicate}");
        let missing_default = glue
            .execute("ALTER TABLE item ADD COLUMN required INTEGER NOT NULL")
            .unwrap_err()
            .to_string();
        assert!(
            missing_default.contains("Default value is required"),
            "{missing_default}"
        );
        assert_eq!(
            select(
                &mut glue,
                "SELECT id, name, qty, note, rating FROM item ORDER BY id"
            ),
            with_rating
        );

        glue.execute("ALTER TABLE item DROP COLUMN qty").unwrap();
        assert_eq!(
            select(
                &mut glue,
                "SELECT id, name, note, rating FROM item ORDER BY id"
            ),
            vec![
                vec![Value::I64(1), text("milk"), text("fresh"), Value::Null],
                vec![Value::I64(2), text("bread"), text("fresh"), Value::Null],
                vec![Value::I64(3), text("eggs"), text("fresh"), Value::Null],
                vec![Value::I64(4), text("tea"), text("loose"), Value::Null],
            ]
        );

        let missing = glue
            .execute("ALTER TABLE item DROP COLUMN qty")
            .unwrap_err()
            .to_string();
        assert!(missing.contains("Dropping column not found"), "{missing}");
        glue.execute("ALTER TABLE item DROP COLUMN IF EXISTS qty")
            .unwrap();

        glue.execute("INSERT INTO item VALUES (5, 'jam', 'seedless', 4)")
            .unwrap();
        assert_eq!(
            select(
                &mut glue,
                "SELECT id, name, note, rating FROM item ORDER BY id"
            ),
            vec![
                vec![Value::I64(1), text("milk"), text("fresh"), Value::Null],
                vec![Value::I64(2), text("bread"), text("fresh"), Value::Null],
                vec![Value::I64(3), text("eggs"), text("fresh"), Value::Null],
                vec![Value::I64(4), text("tea"), text("loose"), Value::Null],
                vec![Value::I64(5), text("jam"), text("seedless"), Value::I64(4)],
            ]
        );
    });

    let reopened = node.docs.open(doc_id).await.unwrap().unwrap();
    let db = GluesqlDb::new(reopened, node.db.blobs().clone(), node.db.author());
    tokio::task::block_in_place(|| {
        let mut glue = Glue::new(db);
        assert_eq!(
            select(
                &mut glue,
                "SELECT id, name, note, rating FROM item ORDER BY id"
            ),
            vec![
                vec![Value::I64(1), text("milk"), text("fresh"), Value::Null],
                vec![Value::I64(2), text("bread"), text("fresh"), Value::Null],
                vec![Value::I64(3), text("eggs"), text("fresh"), Value::Null],
                vec![Value::I64(4), text("tea"), text("loose"), Value::Null],
                vec![Value::I64(5), text("jam"), text("seedless"), Value::I64(4)],
            ]
        );
    });
}
