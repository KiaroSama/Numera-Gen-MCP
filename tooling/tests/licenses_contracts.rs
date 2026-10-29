use numera_task::{context::{Context,sha},licenses::preserve_package};
use serde_json::{Value,json};
use std::{fs,path::{Path,PathBuf}};

struct Fixture { root:PathBuf, package_root:PathBuf, output:PathBuf }
impl Fixture {
    fn new()->Self {
        let root=Path::new(env!("CARGO_MANIFEST_DIR")).join("../.ci-work/tooling-tests").join(format!("notice {}",uuid::Uuid::new_v4()));
        let package_root=root.join(".ci-cache/cargo/registry/src/fixture-1.0.0");fs::create_dir_all(&package_root).unwrap();
        let output=root.join("notices");Self{root,package_root,output}
    }
    fn context(&self)->Context {
        Context{root:self.root.canonicalize().unwrap(),logger:numera_gen_mcp::logging::Logger::new(&self.root.join("logs"),"ERROR")}
    }
    fn package(&self)->Value {json!({"name":"fixture","version":"1.0.0","manifest_path":self.package_root.join("Cargo.toml"),"license":"MIT","license_file":null,"repository":"https://example.invalid/fixture"})}
}
impl Drop for Fixture {fn drop(&mut self){if let Err(error)=fs::remove_dir_all(&self.root){if std::thread::panicking(){eprintln!("Notice fixture cleanup failed: {:?}",error.kind());}else{panic!("Notice fixture cleanup failed: {:?}",error.kind());}}}}

#[test]
fn actual_and_declared_nonstandard_notices_preserve_exact_bytes_and_hashes(){
    let fixture=Fixture::new();let context=fixture.context();let mut package=fixture.package();
    let expected=b"Original copyright and conditions.\n";
    fs::write(fixture.package_root.join("LICENSE-MIT"),expected).unwrap();
    fs::write(fixture.package_root.join("terms.txt"),[expected.as_slice(),b"extra\n"].concat()).unwrap();
    package["license_file"]=json!("terms.txt");
    let result=preserve_package(&context,&package,&fixture.output,&json!({})).unwrap();
    assert_eq!(result["notices"],json!(["LICENSE-MIT","terms.txt"]));
    for name in result["notices"].as_array().unwrap(){
        let name=name.as_str().unwrap();let original=fs::read(fixture.package_root.join(name)).unwrap();
        assert_eq!(fs::read(fixture.output.join("fixture-1.0.0").join(name)).unwrap(),original);
        assert_eq!(result["notice_sha256"][name],sha(&original));
    }
}

#[test]
fn unknown_missing_notice_and_escaping_declared_file_fail_closed(){
    let fixture=Fixture::new();let context=fixture.context();let mut package=fixture.package();
    let error=preserve_package(&context,&package,&fixture.output,&json!({})).unwrap_err();
    assert_eq!(error.to_string(),"Resolved dependency notice missing.");
    assert!(!fixture.output.exists());
    let outside=fixture.root.join("outside.txt");fs::write(&outside,b"not a package notice").unwrap();
    package["license_file"]=json!("../../../../outside.txt");
    assert!(preserve_package(&context,&package,&fixture.output,&json!({})).is_err());
    assert!(!fixture.output.exists());assert_eq!(fs::read(&outside).unwrap(),b"not a package notice");
    // Also exercise an existing escaping file; a missing relative path alone proves no confinement.
    package["license_file"]=json!(outside);
    let error=preserve_package(&context,&package,&fixture.output,&json!({})).unwrap_err();
    assert_eq!(error.to_string(),"Declared notice escapes package.");assert!(!fixture.output.exists());
}

#[test]
fn reviewed_supplement_requires_exact_crate_revision_license_and_notice_hash(){
    let project=Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let supplements:Value=serde_json::from_slice(&fs::read(project.join("docs/licenses/supplements.json")).unwrap()).unwrap();
    let keys=supplements.as_object().unwrap().keys().map(String::as_str).collect::<std::collections::BTreeSet<_>>();
    assert_eq!(keys,std::collections::BTreeSet::from(["alloc-stdlib@0.3.0","rmcp@3.5.1"]));
    for(identity,supplement)in supplements.as_object().unwrap(){
        let fixture=Fixture::new();let context=fixture.context();let mut package=fixture.package();
        let original=fs::read(project.join(supplement["file"].as_str().unwrap())).unwrap();
        let copy=fixture.root.join(supplement["file"].as_str().unwrap());fs::create_dir_all(copy.parent().unwrap()).unwrap();fs::write(&copy,&original).unwrap();
        let(name,version)=identity.split_once('@').unwrap();package["name"]=json!(name);package["version"]=json!(version);package["license"]=supplement["license"].clone();
        fs::write(fixture.package_root.join(".cargo_vcs_info.json"),json!({"git":{"sha1":supplement["source_revision"]}}).to_string()).unwrap();
        fs::write(fixture.package_root.join(".cargo-checksum.json"),json!({"package":supplement["crate_sha256"]}).to_string()).unwrap();
        let result=preserve_package(&context,&package,&fixture.output,&supplements).unwrap();
        assert_eq!(fs::read(fixture.output.join(format!("{name}-{version}")).join("LICENSE")).unwrap(),original);
        assert_eq!(result["supplement"]["source_revision"],supplement["source_revision"]);
        assert_eq!(result["notice_sha256"]["LICENSE"],sha(&original));
        for field in["crate_sha256","source_revision","license","sha256"]{
            let mut changed=supplements.clone();changed[identity][field]=json!("changed");
            let error=preserve_package(&context,&package,&fixture.output,&changed).unwrap_err();
            assert_eq!(error.to_string(),if field=="sha256"{"Supplement hash differs."}else{"Supplement source identity differs."});
        }
        // Independent package provenance corruption must also fail, not merely mutation of expected data.
        fs::write(fixture.package_root.join(".cargo-checksum.json"),json!({"package":"changed"}).to_string()).unwrap();
        assert!(preserve_package(&context,&package,&fixture.output,&supplements).is_err());
    }
}
