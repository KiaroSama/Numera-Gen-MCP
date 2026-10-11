use std::path::{Path,PathBuf};
use serde_json::{Value,json};

pub struct Workspace(pub PathBuf);
impl Workspace {
    pub fn new()->Self {
        let root=Path::new(env!("CARGO_MANIFEST_DIR")).join(".ci-work/native-tests").join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&root).expect("isolated workspace");Self(root)
    }
    pub fn config(&self,base:&str,adapter:&str)->Value {
        json!({"schemaVersion":1,"defaultConnection":"test","outputDir":self.0.join("outputs"),"stateDir":self.0.join("state"),
            "logging":{"directory":self.0.join("logs")},"files":{"allowedInputRoots":[self.0.join("inputs")]},
            "connections":{"test":{"adapter":adapter,"baseUrl":base,"auth":{"type":"none"},"defaultModel":"opaque/image","orchestrationModel":"opaque/orchestrator","requestTimeoutMs":3000}}})
    }
    pub fn image()->Vec<u8> {
        let file=Path::new(env!("CARGO_MANIFEST_DIR")).join(".ci-work/parity/images.json");
        let value:Value=serde_json::from_slice(&std::fs::read(file).expect("source image oracle")).unwrap();
        numera_image_gen_mcp::files::images::decode_base64(value[0]["base64"].as_str().unwrap(),1048576).unwrap()
    }
}
impl Drop for Workspace {fn drop(&mut self){let _=std::fs::remove_dir_all(&self.0);}}
