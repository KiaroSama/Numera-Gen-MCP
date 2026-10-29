mod common;
use common::Workspace;
use numera_gen_mcp::compat::value::json;
use numera_gen_mcp::{
    compat::hash::sha256,
    jobs::{Receipt, Store},
};
use rusqlite::Connection;

#[test]
fn legacy_wal_snapshot_preserves_receipts_and_refuses_future_or_incomplete_schemas() {
    let workspace = Workspace::new();
    let directory = workspace.0.join("legacy");
    std::fs::create_dir(&directory).unwrap();
    let database = directory.join("receipts.sqlite");
    let legacy = Connection::open(&database).unwrap();
    legacy.execute_batch("PRAGMA journal_mode=WAL; PRAGMA user_version=1; CREATE TABLE requests(id TEXT PRIMARY KEY,fingerprint TEXT NOT NULL,identity TEXT NOT NULL,receipt TEXT NOT NULL,owner TEXT,lease INTEGER,cancel INTEGER NOT NULL DEFAULT 0);").unwrap();
    let value = json!({"request_id":"existing","connection":"test","status":"outcome_unknown","generation_outcome":"unknown","another_submission_may_charge":true,"outputs":[],"warnings":[],"errors":[],"deviations":[]});
    legacy
        .execute(
            "INSERT INTO requests VALUES(?1,?2,?3,?4,NULL,0,0)",
            rusqlite::params!["existing", "fingerprint", "account", value.to_string()],
        )
        .unwrap();
    let mut native = Store::open(&directory).unwrap();
    assert_eq!(native.get("existing").unwrap().value, value);
    assert_eq!(native.identity("existing").unwrap(), "account");
    assert!(
        !native
            .prepare(Receipt::new(value.clone()), "fingerprint", "account")
            .unwrap()
            .0
    );
    assert_eq!(
        native
            .prepare(Receipt::new(value.clone()), "changed", "account")
            .unwrap_err()
            .code,
        "request_id_conflict"
    );
    let snapshots = std::fs::read_dir(&directory)
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("receipts-before-journal-")
                && p.extension().is_some_and(|x| x == "sqlite")
        })
        .collect::<Vec<_>>();
    assert_eq!(snapshots.len(), 1);
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(snapshots[0].with_extension("sqlite.json")).unwrap())
            .unwrap();
    assert_eq!(
        manifest["sha256"],
        sha256(&std::fs::read(&snapshots[0]).unwrap())
    );
    let backup = Connection::open(&snapshots[0]).unwrap();
    assert_eq!(
        backup
            .query_row(
                "SELECT receipt FROM requests WHERE id='existing'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        value.to_string()
    );
    drop(backup);
    drop(native);
    drop(legacy);
    for (name, ddl) in [
        ("future", "PRAGMA user_version=3;"),
        (
            "incomplete",
            "CREATE TABLE requests(id TEXT, generation INTEGER);",
        ),
    ] {
        let directory = workspace.0.join(name);
        std::fs::create_dir(&directory).unwrap();
        let db = Connection::open(directory.join("receipts.sqlite")).unwrap();
        db.execute_batch(ddl).unwrap();
        drop(db);
        assert_eq!(
            Store::open(&directory).err().unwrap().code,
            "invalid_configuration"
        );
    }
}

#[test]
fn media_overflow_extension_preserves_existing_schema2_rows_and_legacy_table_bytes() {
    let workspace = Workspace::new();
    let directory = workspace.0.join("existing schema2");
    std::fs::create_dir(&directory).unwrap();
    let database = directory.join("receipts.sqlite");
    let legacy = Connection::open(&database).unwrap();
    legacy.execute_batch("PRAGMA user_version=2;
        CREATE TABLE requests(id TEXT PRIMARY KEY,fingerprint TEXT NOT NULL,identity TEXT NOT NULL,receipt TEXT NOT NULL,owner TEXT,lease INTEGER,cancel INTEGER NOT NULL DEFAULT 0,generation INTEGER NOT NULL DEFAULT 1,reservation INTEGER NOT NULL DEFAULT 0);
        CREATE TABLE outputs(id TEXT PRIMARY KEY,request_id TEXT NOT NULL,metadata TEXT NOT NULL);
        CREATE TABLE continuation(id TEXT PRIMARY KEY,scope TEXT NOT NULL,value TEXT NOT NULL);
        CREATE TABLE result_journal(request_id TEXT NOT NULL,item INTEGER NOT NULL CHECK(item>=0 AND item<10),output_id TEXT NOT NULL UNIQUE,target TEXT,bytes BLOB,url TEXT,sha256 TEXT,phase TEXT NOT NULL,PRIMARY KEY(request_id,item));").unwrap();
    let receipt = "{\"request_id\":\"old\",\"status\":\"outcome_unknown\",\"generation_outcome\":\"unknown\",\"outputs\":[],\"errors\":[],\"deviations\":[],\"note\":\"تصویر\"}";
    legacy.execute("INSERT INTO requests(id,fingerprint,identity,receipt) VALUES('old','hash','account',?)", [receipt]).unwrap();
    legacy.execute("INSERT INTO outputs VALUES('old-output','old','{\"retained\":true}')", []).unwrap();
    legacy.execute("INSERT INTO continuation VALUES('old','account','{\"handle\":\"original\"}')", []).unwrap();
    legacy.execute("INSERT INTO result_journal(request_id,item,output_id,bytes,sha256,phase) VALUES('old',9,'pending',?,'original','retained')", [b"original bytes".as_slice()]).unwrap();
    let schema: String = legacy.query_row("SELECT sql FROM sqlite_schema WHERE name='result_journal'", [], |row| row.get(0)).unwrap();
    let current = Store::open(&directory).unwrap();
    assert_eq!(current.identity("old").unwrap(), "account");
    assert_eq!(current.pending("old").unwrap()[0].bytes.as_deref(), Some(b"original bytes".as_slice()));
    assert_eq!(current.continuation("old", "account").unwrap(), json!({"handle":"original"}));
    assert_eq!(current.list_outputs(0, 10).unwrap(), vec![json!({"retained":true})]);
    assert_eq!(legacy.query_row("SELECT receipt FROM requests WHERE id='old'", [], |row| row.get::<_, String>(0)).unwrap(), receipt);
    assert_eq!(legacy.query_row("SELECT sql FROM sqlite_schema WHERE name='result_journal'", [], |row| row.get::<_, String>(0)).unwrap(), schema);
    assert_eq!(legacy.query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0)).unwrap(), 2);
    assert!(legacy.execute("INSERT INTO result_journal(request_id,item,output_id,phase) VALUES('old',10,'forbidden','waiting')", []).is_err());
    drop(current);
    let reopened = Store::open(&directory).unwrap();
    assert_eq!(reopened.pending("old").unwrap().len(), 1);
    assert_eq!(legacy.query_row("SELECT receipt FROM requests WHERE id='old'", [], |row| row.get::<_, String>(0)).unwrap(), receipt);
}
