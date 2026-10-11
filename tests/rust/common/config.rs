use crate::common::Workspace;
use serde_json::{Value, json};

impl Workspace {
    pub fn config(&self, base: &str, adapter: &str) -> Value {
        json!({"schemaVersion":1,"defaultConnection":"test","outputDir":self.0.join("outputs"),"stateDir":self.0.join("state"),
            "logging":{"directory":self.0.join("logs")},"files":{"allowedInputRoots":[self.0.join("inputs")]},
            "connections":{"test":{"adapter":adapter,"baseUrl":base,"auth":{"type":"none"},"defaultModel":"opaque/image","orchestrationModel":"opaque/orchestrator","requestTimeoutMs":3000}}})
    }
}
