// These slop tests seem to be completely broken. I'll fix if I feel like it

// use std::time::{Duration, Instant};

// use gluesql::core::{
//     data::Value,
//     error::Error,
//     prelude::{Glue, Payload},
// };
// use iroh::{Endpoint, EndpointAddr, endpoint::presets, protocol::Router};
// use iroh_blobs::{BlobsProtocol, store::mem::MemStore};
// use iroh_docs::{
//     api::{
//         Doc,
//         protocol::{AddrInfoOptions, ShareMode},
//     },
//     protocol::Docs,
// };
// use iroh_gossip::net::Gossip;
// use shoppinglist_db::GluesqlDb;
// use tokio::time::sleep;

// const NOTES: &str = "SELECT id, author, body FROM notes ORDER BY id";
// const CREATE_NOTES: &str = "CREATE TABLE notes (id INTEGER PRIMARY KEY, author TEXT, body TEXT)";
// const SYNC_TIMEOUT: Duration = Duration::from_secs(30);
// const ISOLATION: Duration = Duration::from_secs(1);
// const POLL: Duration = Duration::from_millis(40);

// // Iroh seems to get stuck sometimes with too much stuff going on on the same machine.
// async fn test_mutex() -> tokio::sync::MutexGuard<'static, ()> {
//     static SLOT: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
//     SLOT.get_or_init(|| tokio::sync::Mutex::new(()))
//         .lock()
//         .await
// }

// struct Stack {
//     name: &'static str,
//     endpoint: Endpoint,
//     blobs: iroh_blobs::api::Store,
//     mem: MemStore,
//     router: Router,
//     docs: Docs,
//     author: iroh_docs::AuthorId,
// }

// struct User {
//     name: &'static str,
//     endpoint: Endpoint,
//     db: GluesqlDb,
//     _docs: Docs,
//     _blobs: MemStore,
//     router: Router,
// }

// impl Stack {
//     fn into_user(self, doc: Doc) -> User {
//         let db = GluesqlDb::new(doc, self.blobs, self.author);
//         User {
//             name: self.name,
//             endpoint: self.endpoint,
//             db,
//             _docs: self.docs,
//             _blobs: self.mem,
//             router: self.router,
//         }
//     }
// }

// async fn stack(name: &'static str) -> Stack {
//     let endpoint = Endpoint::bind(presets::Minimal).await.unwrap();
//     let mem = MemStore::new();
//     let blobs = (*mem).clone();
//     let gossip = Gossip::builder().spawn(endpoint.clone());
//     let docs = Docs::memory()
//         .spawn(endpoint.clone(), blobs.clone(), gossip.clone())
//         .await
//         .unwrap();
//     let router = Router::builder(endpoint.clone())
//         .accept(iroh_blobs::ALPN, BlobsProtocol::new(&mem, None))
//         .accept(iroh_gossip::ALPN, gossip)
//         .accept(iroh_docs::ALPN, docs.clone())
//         .spawn();
//     let author = docs.author_create().await.unwrap();
//     Stack {
//         name,
//         endpoint,
//         blobs,
//         mem,
//         router,
//         docs,
//         author,
//     }
// }

// async fn cluster(names: &[&'static str]) -> Vec<User> {
//     let mut names = names.iter().copied();
//     let alice = stack(names.next().expect("cluster needs a user")).await;
//     let doc = alice.docs.create().await.unwrap();
//     let ticket = doc
//         .share(ShareMode::Write, AddrInfoOptions::Addresses)
//         .await
//         .unwrap();
//     assert!(
//         ticket.nodes.iter().any(|addr| !addr.is_empty()),
//         "share ticket has no direct address"
//     );
//     let mut users = vec![alice.into_user(doc)];
//     for name in names {
//         let peer = stack(name).await;
//         let doc = peer
//             .docs
//             .import_namespace(ticket.capability.clone())
//             .await
//             .unwrap();
//         users.push(peer.into_user(doc));
//     }
//     let refs: Vec<&User> = users.iter().collect();
//     connect_all(&refs).await;
//     users
// }

// async fn finish(users: Vec<User>) {
//     for user in users {
//         user.router.shutdown().await.unwrap();
//     }
// }

// async fn later() {
//     sleep(Duration::from_millis(50)).await;
// }

// async fn dialable(user: &User) -> EndpointAddr {
//     let deadline = Instant::now() + Duration::from_secs(5);
//     loop {
//         let addr = user.endpoint.addr();
//         if !addr.is_empty() {
//             return addr;
//         }
//         if Instant::now() >= deadline {
//             panic!("{} has no direct address", user.name);
//         }
//         sleep(Duration::from_millis(20)).await;
//     }
// }

// async fn peer_addrs(user: &User, peers: &[&User]) -> Vec<EndpointAddr> {
//     let mut addrs = Vec::new();
//     for peer in peers {
//         if peer.endpoint.id() != user.endpoint.id() {
//             addrs.push(dialable(peer).await);
//         }
//     }
//     addrs
// }

// async fn go_offline(user: &User) {
//     user.db.doc().leave().await.unwrap();
//     let status = user.db.doc().status().await.unwrap();
//     assert!(!status.sync, "{} still has sync enabled", user.name);
// }

// async fn go_online(user: &User, peers: &[&User]) {
//     user.db
//         .doc()
//         .start_sync(peer_addrs(user, peers).await)
//         .await
//         .unwrap();
//     let status = user.db.doc().status().await.unwrap();
//     assert!(status.sync, "{} did not enable sync", user.name);
// }

// async fn connect_all(users: &[&User]) {
//     for user in users {
//         go_online(user, users).await;
//     }
// }

// fn note(id: i64, author: &str, body: &str) -> Vec<Value> {
//     vec![
//         Value::I64(id),
//         Value::Str(author.to_owned()),
//         Value::Str(body.to_owned()),
//     ]
// }

// fn exec_db(db: GluesqlDb, sql: &str) {
//     Glue::new(db)
//         .execute(sql)
//         .unwrap_or_else(|err| panic!("`{sql}` failed: {err}"));
// }

// fn exec(user: &User, sql: &str) {
//     exec_db(user.db.clone(), sql);
// }

// fn try_query(user: &User, sql: &str) -> Result<Vec<Vec<Value>>, Error> {
//     let mut payloads = Glue::new(user.db.clone()).execute(sql)?;
//     match payloads.pop() {
//         Some(Payload::Select { rows, .. }) => Ok(rows),
//         Some(other) => panic!("{} `{sql}` returned {other:?}", user.name),
//         None => panic!("{} `{sql}` returned no payload", user.name),
//     }
// }

// fn query(user: &User, sql: &str) -> Vec<Vec<Value>> {
//     try_query(user, sql).unwrap_or_else(|err| panic!("{} `{sql}`: {err}", user.name))
// }

// async fn at_once(jobs: Vec<(GluesqlDb, String)>) {
//     let mut tasks = Vec::new();
//     for (db, sql) in jobs {
//         tasks.push(tokio::spawn(async move { exec_db(db, &sql) }));
//     }
//     for task in tasks {
//         task.await.unwrap();
//     }
// }

// async fn await_rows(user: &User, sql: &str, expected: &[Vec<Value>]) {
//     let deadline = Instant::now() + SYNC_TIMEOUT;
//     let mut last = String::new();
//     while Instant::now() < deadline {
//         match try_query(user, sql) {
//             Ok(rows) if rows == expected => return,
//             Ok(rows) => last = format!("{rows:?}"),
//             Err(err) => last = err.to_string(),
//         }
//         sleep(POLL).await;
//     }
//     panic!(
//         "{} did not reach {expected:?} within {SYNC_TIMEOUT:?}; last: {last}",
//         user.name
//     );
// }

// async fn await_all(users: &[&User], sql: &str, expected: &[Vec<Value>]) {
//     for user in users {
//         await_rows(user, sql, expected).await;
//     }
// }

// async fn await_missing_table(user: &User, sql: &str) {
//     let deadline = Instant::now() + SYNC_TIMEOUT;
//     let mut last = String::from("query succeeded");
//     while Instant::now() < deadline {
//         match try_query(user, sql) {
//             Err(err) if err.to_string().contains("table not found") => return,
//             Err(err) => last = err.to_string(),
//             Ok(rows) => last = format!("rows {rows:?}"),
//         }
//         sleep(POLL).await;
//     }
//     panic!("{} can still query the table; last: {last}", user.name);
// }

// async fn assert_unchanged(user: &User, sql: &str, expected: &[Vec<Value>]) {
//     let start = Instant::now();
//     while start.elapsed() < ISOLATION {
//         assert_eq!(
//             query(user, sql),
//             expected,
//             "{} changed while sync was off",
//             user.name
//         );
//         sleep(POLL).await;
//     }
// }

// #[tokio::test(flavor = "multi_thread")]
// async fn while_everyone_is_online_distinct_rows_merge_and_one_row_converges() {
//     let _slot = test_mutex().await;
//     let users = cluster(&["alice", "bob", "carol"]).await;
//     let alice = &users[0];
//     let bob = &users[1];
//     let carol = &users[2];
//     let everyone = [alice, bob, carol];

//     exec(alice, CREATE_NOTES);
//     exec(alice, "INSERT INTO notes VALUES (1, 'alice', 'original')");
//     let original = [note(1, "alice", "original")];
//     await_all(&everyone, NOTES, &original).await;

//     at_once(vec![
//         (
//             bob.db.clone(),
//             "INSERT INTO notes VALUES (2, 'bob', 'second')".to_owned(),
//         ),
//         (
//             carol.db.clone(),
//             "INSERT INTO notes VALUES (3, 'carol', 'third')".to_owned(),
//         ),
//     ])
//     .await;
//     let written = [
//         note(1, "alice", "original"),
//         note(2, "bob", "second"),
//         note(3, "carol", "third"),
//     ];
//     await_all(&everyone, NOTES, &written).await;

//     exec(
//         alice,
//         "UPDATE notes SET author = 'alice', body = 'from-alice' WHERE id = 1",
//     );
//     later().await;
//     exec(
//         bob,
//         "UPDATE notes SET author = 'bob', body = 'from-bob' WHERE id = 1",
//     );
//     later().await;
//     exec(
//         carol,
//         "UPDATE notes SET author = 'carol', body = 'from-carol' WHERE id = 1",
//     );
//     await_all(
//         &everyone,
//         NOTES,
//         &[
//             note(1, "carol", "from-carol"),
//             note(2, "bob", "second"),
//             note(3, "carol", "third"),
//         ],
//     )
//     .await;

//     finish(users).await;
// }

// #[tokio::test(flavor = "multi_thread")]
// async fn a_peer_who_leaves_keeps_a_stale_view_until_it_rejoins() {
//     let _slot = test_mutex().await;
//     let users = cluster(&["alice", "bob", "carol"]).await;
//     let alice = &users[0];
//     let bob = &users[1];
//     let carol = &users[2];
//     let everyone = [alice, bob, carol];

//     exec(alice, CREATE_NOTES);
//     exec(alice, "INSERT INTO notes VALUES (1, 'seed', 'seed')");
//     let seed = [note(1, "seed", "seed")];
//     await_all(&everyone, NOTES, &seed).await;

//     go_offline(bob).await;
//     exec(
//         alice,
//         "UPDATE notes SET author = 'alice', body = 'from-alice' WHERE id = 1",
//     );
//     exec(carol, "INSERT INTO notes VALUES (3, 'carol', 'from-carol')");
//     let live = [
//         note(1, "alice", "from-alice"),
//         note(3, "carol", "from-carol"),
//     ];
//     await_all(&[alice, carol], NOTES, &live).await;
//     assert_unchanged(bob, NOTES, &seed).await;

//     exec(bob, "INSERT INTO notes VALUES (2, 'bob', 'from-bob')");
//     let bob_local = [note(1, "seed", "seed"), note(2, "bob", "from-bob")];
//     assert_unchanged(bob, NOTES, &bob_local).await;
//     assert_unchanged(alice, NOTES, &live).await;

//     go_online(bob, &everyone).await;
//     let merged = [
//         note(1, "alice", "from-alice"),
//         note(2, "bob", "from-bob"),
//         note(3, "carol", "from-carol"),
//     ];
//     await_all(&everyone, NOTES, &merged).await;

//     finish(users).await;
// }

// #[tokio::test(flavor = "multi_thread")]
// async fn offline_edits_merge_when_peers_rejoin_and_the_later_write_wins() {
//     let _slot = test_mutex().await;
//     let users = cluster(&["alice", "bob", "carol"]).await;
//     let alice = &users[0];
//     let bob = &users[1];
//     let carol = &users[2];
//     let everyone = [alice, bob, carol];

//     exec(alice, CREATE_NOTES);
//     exec(alice, "INSERT INTO notes VALUES (1, 'seed', 'seed')");
//     let seed = [note(1, "seed", "seed")];
//     await_all(&everyone, NOTES, &seed).await;

//     go_offline(alice).await;
//     go_offline(bob).await;
//     go_offline(carol).await;

//     exec(
//         alice,
//         "UPDATE notes SET author = 'alice', body = 'alice' WHERE id = 1",
//     );
//     exec(alice, "INSERT INTO notes VALUES (5, 'alice', 'early')");
//     later().await;
//     exec(
//         bob,
//         "UPDATE notes SET author = 'bob', body = 'bob' WHERE id = 1",
//     );
//     exec(bob, "INSERT INTO notes VALUES (5, 'bob', 'later')");
//     exec(carol, "INSERT INTO notes VALUES (4, 'carol', 'apart')");

//     let alice_local = [note(1, "alice", "alice"), note(5, "alice", "early")];
//     let bob_local = [note(1, "bob", "bob"), note(5, "bob", "later")];
//     let carol_local = [note(1, "seed", "seed"), note(4, "carol", "apart")];
//     assert_eq!(query(alice, NOTES), alice_local);
//     assert_eq!(query(bob, NOTES), bob_local);
//     assert_eq!(query(carol, NOTES), carol_local);

//     go_online(alice, &[alice, bob]).await;
//     go_online(bob, &[alice, bob]).await;
//     let alice_and_bob = [note(1, "bob", "bob"), note(5, "bob", "later")];
//     await_all(&[alice, bob], NOTES, &alice_and_bob).await;
//     assert_unchanged(carol, NOTES, &carol_local).await;

//     go_online(carol, &everyone).await;
//     let merged = [
//         note(1, "bob", "bob"),
//         note(4, "carol", "apart"),
//         note(5, "bob", "later"),
//     ];
//     await_all(&everyone, NOTES, &merged).await;

//     finish(users).await;
// }

// #[tokio::test(flavor = "multi_thread")]
// async fn the_later_of_a_delete_and_an_update_is_the_row_that_remains() {
//     let _slot = test_mutex().await;
//     let users = cluster(&["alice", "bob"]).await;
//     let alice = &users[0];
//     let bob = &users[1];
//     let both = [alice, bob];

//     exec(alice, CREATE_NOTES);
//     exec(
//         alice,
//         "INSERT INTO notes VALUES (1, 'seed', 'one'), (2, 'seed', 'two')",
//     );
//     let seed = [note(1, "seed", "one"), note(2, "seed", "two")];
//     await_all(&both, NOTES, &seed).await;

//     go_offline(alice).await;
//     go_offline(bob).await;

//     exec(alice, "DELETE FROM notes WHERE id = 1");
//     later().await;
//     exec(
//         bob,
//         "UPDATE notes SET author = 'bob', body = 'revived' WHERE id = 1",
//     );
//     exec(
//         bob,
//         "UPDATE notes SET author = 'bob', body = 'first' WHERE id = 2",
//     );
//     later().await;
//     exec(alice, "DELETE FROM notes WHERE id = 2");

//     assert_eq!(query(alice, NOTES), Vec::<Vec<Value>>::new());
//     assert_eq!(
//         query(bob, NOTES),
//         [note(1, "bob", "revived"), note(2, "bob", "first")]
//     );

//     connect_all(&both).await;
//     await_all(&both, NOTES, &[note(1, "bob", "revived")]).await;

//     finish(users).await;
// }

// #[tokio::test(flavor = "multi_thread")]
// async fn offline_appends_keep_every_row() {
//     let _slot = test_mutex().await;
//     let users = cluster(&["alice", "bob"]).await;
//     let alice = &users[0];
//     let bob = &users[1];
//     let both = [alice, bob];

//     exec(alice, "CREATE TABLE log (note TEXT)");
//     await_all(&both, "SELECT note FROM log", &[]).await;

//     go_offline(alice).await;
//     go_offline(bob).await;

//     exec(alice, "INSERT INTO log VALUES ('a1'), ('a2')");
//     exec(bob, "INSERT INTO log VALUES ('b1')");

//     assert_eq!(sorted_notes(alice), ["a1", "a2"]);
//     assert_eq!(sorted_notes(bob), ["b1"]);

//     connect_all(&both).await;
//     await_notes(alice, &["a1", "a2", "b1"]).await;
//     await_notes(bob, &["a1", "a2", "b1"]).await;

//     exec(alice, "INSERT INTO log VALUES ('c')");
//     await_notes(alice, &["a1", "a2", "b1", "c"]).await;
//     await_notes(bob, &["a1", "a2", "b1", "c"]).await;

//     finish(users).await;
// }

// #[tokio::test(flavor = "multi_thread")]
// async fn a_peer_that_joins_after_the_writes_sees_them() {
//     let _slot = test_mutex().await;
//     let mut users = cluster(&["alice", "bob"]).await;
//     let alice = &users[0];
//     let bob = &users[1];

//     exec(alice, CREATE_NOTES);
//     exec(alice, "INSERT INTO notes VALUES (1, 'alice', 'one')");
//     await_rows(bob, NOTES, &[note(1, "alice", "one")]).await;
//     exec(bob, "INSERT INTO notes VALUES (2, 'bob', 'two')");
//     let rows = [note(1, "alice", "one"), note(2, "bob", "two")];
//     await_all(&[alice, bob], NOTES, &rows).await;

//     let carol_stack = stack("carol").await;
//     let ticket = alice
//         .db
//         .doc()
//         .share(ShareMode::Write, AddrInfoOptions::Addresses)
//         .await
//         .unwrap();
//     let doc = carol_stack
//         .docs
//         .import_namespace(ticket.capability)
//         .await
//         .unwrap();
//     let carol = carol_stack.into_user(doc);
//     let err = try_query(&carol, NOTES).unwrap_err();
//     assert!(
//         err.to_string().contains("table not found"),
//         "carol queried notes before sync: {err}"
//     );

//     let alice = &users[0];
//     let bob = &users[1];
//     go_online(&carol, &[alice, bob, &carol]).await;
//     await_all(&[alice, bob, &carol], NOTES, &rows).await;

//     users.push(carol);
//     finish(users).await;
// }

// #[tokio::test(flavor = "multi_thread")]
// async fn a_table_created_while_a_peer_is_offline_appears_when_that_peer_rejoins() {
//     let _slot = test_mutex().await;
//     let users = cluster(&["alice", "bob"]).await;
//     let alice = &users[0];
//     let bob = &users[1];
//     let both = [alice, bob];

//     exec(alice, "CREATE TABLE marker (id INTEGER PRIMARY KEY)");
//     exec(alice, "INSERT INTO marker VALUES (1)");
//     await_all(&both, "SELECT id FROM marker", &[vec![Value::I64(1)]]).await;

//     go_offline(bob).await;
//     exec(alice, CREATE_NOTES);
//     exec(
//         alice,
//         "INSERT INTO notes VALUES (1, 'alice', 'while-bob-was-away')",
//     );
//     let err = try_query(bob, NOTES).unwrap_err();
//     assert!(
//         err.to_string().contains("table not found"),
//         "bob saw notes while offline: {err}"
//     );

//     go_online(bob, &both).await;
//     await_all(&both, NOTES, &[note(1, "alice", "while-bob-was-away")]).await;

//     finish(users).await;
// }

// #[tokio::test(flavor = "multi_thread")]
// async fn adding_and_dropping_a_column_reaches_the_other_peer() {
//     let _slot = test_mutex().await;
//     let users = cluster(&["alice", "bob"]).await;
//     let alice = &users[0];
//     let bob = &users[1];
//     let both = [alice, bob];

//     exec(
//         alice,
//         "CREATE TABLE item (id INTEGER PRIMARY KEY, name TEXT);
//          INSERT INTO item VALUES (1, 'milk'), (2, 'bread')",
//     );
//     let original = [
//         vec![Value::I64(1), Value::Str("milk".to_owned())],
//         vec![Value::I64(2), Value::Str("bread".to_owned())],
//     ];
//     await_all(&both, "SELECT id, name FROM item ORDER BY id", &original).await;

//     exec(
//         alice,
//         "ALTER TABLE item ADD COLUMN note TEXT DEFAULT 'fresh'",
//     );
//     let with_note = [
//         vec![
//             Value::I64(1),
//             Value::Str("milk".to_owned()),
//             Value::Str("fresh".to_owned()),
//         ],
//         vec![
//             Value::I64(2),
//             Value::Str("bread".to_owned()),
//             Value::Str("fresh".to_owned()),
//         ],
//     ];
//     await_all(
//         &both,
//         "SELECT id, name, note FROM item ORDER BY id",
//         &with_note,
//     )
//     .await;

//     exec(
//         bob,
//         "INSERT INTO item (id, name, note) VALUES (3, 'eggs', 'dozen')",
//     );
//     let with_eggs = [
//         with_note[0].clone(),
//         with_note[1].clone(),
//         vec![
//             Value::I64(3),
//             Value::Str("eggs".to_owned()),
//             Value::Str("dozen".to_owned()),
//         ],
//     ];
//     await_all(
//         &both,
//         "SELECT id, name, note FROM item ORDER BY id",
//         &with_eggs,
//     )
//     .await;

//     exec(alice, "ALTER TABLE item DROP COLUMN note");
//     let without_note = [
//         vec![Value::I64(1), Value::Str("milk".to_owned())],
//         vec![Value::I64(2), Value::Str("bread".to_owned())],
//         vec![Value::I64(3), Value::Str("eggs".to_owned())],
//     ];
//     await_all(
//         &both,
//         "SELECT id, name FROM item ORDER BY id",
//         &without_note,
//     )
//     .await;

//     finish(users).await;
// }

// #[tokio::test(flavor = "multi_thread")]
// async fn dropping_a_table_leaves_rows_a_peer_inserted_while_offline() {
//     let _slot = test_mutex().await;
//     let users = cluster(&["alice", "bob"]).await;
//     let alice = &users[0];
//     let bob = &users[1];
//     let both = [alice, bob];

//     exec(alice, CREATE_NOTES);
//     exec(alice, "INSERT INTO notes VALUES (1, 'seed', 'seed')");
//     let seed = [note(1, "seed", "seed")];
//     await_all(&both, NOTES, &seed).await;

//     go_offline(bob).await;
//     exec(alice, "DROP TABLE notes");
//     await_missing_table(alice, NOTES).await;
//     assert_unchanged(bob, NOTES, &seed).await;

//     exec(
//         bob,
//         "INSERT INTO notes VALUES (2, 'bob', 'written-offline')",
//     );
//     let bob_local = [note(1, "seed", "seed"), note(2, "bob", "written-offline")];
//     assert_eq!(query(bob, NOTES), bob_local);

//     go_online(bob, &both).await;
//     await_missing_table(alice, NOTES).await;
//     await_missing_table(bob, NOTES).await;

//     exec(alice, CREATE_NOTES);
//     await_all(&both, NOTES, &[note(2, "bob", "written-offline")]).await;

//     finish(users).await;
// }

// fn sorted_notes(user: &User) -> Vec<String> {
//     let mut notes: Vec<String> = query(user, "SELECT note FROM log")
//         .into_iter()
//         .map(|row| match &row[0] {
//             Value::Str(note) => note.clone(),
//             other => panic!("note column was {other:?}"),
//         })
//         .collect();
//     notes.sort();
//     notes
// }

// async fn await_notes(user: &User, expected: &[&str]) {
//     let expected: Vec<String> = expected.iter().map(|note| (*note).to_owned()).collect();
//     let deadline = Instant::now() + SYNC_TIMEOUT;
//     let mut last = String::new();
//     while Instant::now() < deadline {
//         match try_query(user, "SELECT note FROM log") {
//             Ok(rows) => {
//                 let mut notes: Vec<String> = rows
//                     .into_iter()
//                     .map(|row| match &row[0] {
//                         Value::Str(note) => note.clone(),
//                         other => panic!("note column was {other:?}"),
//                     })
//                     .collect();
//                 notes.sort();
//                 if notes == expected {
//                     return;
//                 }
//                 last = format!("{notes:?}");
//             }
//             Err(err) => last = err.to_string(),
//         }
//         sleep(POLL).await;
//     }
//     panic!(
//         "{} notes did not become {expected:?} within {SYNC_TIMEOUT:?}; last: {last}",
//         user.name
//     );
// }
