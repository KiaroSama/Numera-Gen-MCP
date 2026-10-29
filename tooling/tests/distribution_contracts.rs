use numera_task::{archives, context::sha, distribution::{authenticate_archive, prefix, validate_entries, write_tree}};
use serde_json::json;
use std::{collections::BTreeMap, path::Path};

fn fixture() -> BTreeMap<String,Vec<u8>> {
    let mut entries=BTreeMap::from([
        ("bin/numera-gen-mcp.exe".into(), b"synthetic never executed".to_vec()),
        ("bin/numera-dev.exe".into(), b"synthetic developer".to_vec()),
        ("docs/assets/logo.png".into(), numera_gen_mcp::build_identity::LOGO.to_vec()),
    ]);
    let manifest=json!({"version":"0.3.0","target":"x86_64-pc-windows-msvc","source_sha":"a".repeat(40),"publisher":"KiaroSama",
        "binary_sha256":sha(&entries["bin/numera-gen-mcp.exe"]),"files":entries.iter().map(|(name,data)|json!({"path":name,"bytes":data.len(),"sha256":sha(data)})).collect::<Vec<_>>()});
    entries.insert("manifest.json".into(),serde_json::to_vec(&manifest).unwrap());entries
}
#[test]
fn archive_checksum_identity_and_complete_inventory_precede_staging() {
    let entries=fixture();let target="x86_64-pc-windows-msvc";let source="a".repeat(40);
    let prefix=prefix(target,true).unwrap();let name=format!("{prefix}.zip");
    let bytes=archives::encode(&prefix,&entries,true).unwrap();
    let sidecar=format!("{}  {name}\n",sha(&bytes));
    assert_eq!(authenticate_archive(&bytes,sidecar.as_bytes(),target,&source,true).unwrap(),entries);
    for changed in [format!("{}  wrong.zip\n",sha(&bytes)),format!("{}  {name}\n","0".repeat(64))] {
        assert!(authenticate_archive(&bytes,changed.as_bytes(),target,&source,true).is_err());
    }
    assert!(authenticate_archive(&bytes,sidecar.as_bytes(),target,&"b".repeat(40),true).is_err());
    let mut signed=entries.clone();signed.insert("manifest.sig".into(),vec![0;384]);
    let signed=archives::encode(&prefix,&signed,true).unwrap();
    assert!(authenticate_archive(&signed,format!("{} {name}",sha(&signed)).as_bytes(),target,&source,true).is_err());
    assert!(authenticate_archive(&bytes,sidecar.as_bytes(),target,&source,false).is_err());
}

#[test]
fn complete_inventory_is_authenticated_before_any_staging_write() {
    let original=fixture();let target="x86_64-pc-windows-msvc";let source="a".repeat(40);
    validate_entries(&original,target,&source).unwrap();
    for name in ["secrets.md","docs/extra.txt","bin/numera-dev.exe"] {
        let mut changed=original.clone();changed.insert(name.into(),b"changed".to_vec());
        assert!(validate_entries(&changed,target,&source).is_err());
    }
    assert!(validate_entries(&original,target,&"b".repeat(40)).is_err());
    let root=Path::new(env!("CARGO_MANIFEST_DIR")).join("../.ci-work/tooling-tests").join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&root).unwrap();let prepared=root.join("prepared");
    let result=std::panic::catch_unwind(||{
        write_tree(&prepared,&original).unwrap();
        assert_eq!(numera_task::distribution::read_tree(&prepared,target,&source).unwrap(),original);
        assert_eq!(std::fs::read(prepared.join("bin/numera-dev.exe")).unwrap(),original["bin/numera-dev.exe"]);
        assert!(write_tree(&prepared,&original).is_err());
        std::fs::write(prepared.join("docs/unlisted.txt"),b"not authenticated").unwrap();
        assert!(numera_task::distribution::read_tree(&prepared,target,&source).is_err());
        std::fs::remove_file(prepared.join("docs/unlisted.txt")).unwrap();
        let excessive=prepared.join("extra");std::fs::create_dir(&excessive).unwrap();
        assert!(numera_task::distribution::read_tree(&prepared,target,&source).is_err());
    });
    std::fs::remove_dir_all(root).unwrap();result.unwrap();
}
