//! Fixed first-party Cargo output boundary; ordinary artifact reads remain single-link.
use crate::context::{Context, fail};
use numera_gen_mcp::devtools::cargo_output::{self, Binary};
use std::io;

pub fn read(context: &Context, name: &str) -> io::Result<Vec<u8>> {
    let binary = match name {
        "numera-gen-mcp" => Binary::Server,
        "numera-dev" => Binary::Developer,
        _ => return Err(fail("Unknown first-party Cargo release output.")),
    };
    cargo_output::read(&context.root, binary)
        .map_err(|_| fail("Trusted Cargo release output failed identity or stability checks."))
}
