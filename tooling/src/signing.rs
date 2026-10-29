use crate::{context::{Context,fail,sha},distribution::{self,read_regular},private_files::{PrivateInputs,Secret}};
use base64::{Engine,engine::general_purpose::STANDARD};
use numera_gen_mcp::devtools::artifact;
use serde_json::{Value,json};
use std::{fs,io,path::{Path,PathBuf},process::Command};

pub const CERT_PIN:&str="657dd7869e90e2bf9a49942d5098e1e3a82e22cf84add51aa958e3dfd0af937f";
pub fn public_identity()->io::Result<Value>{
    let identity:Value=serde_json::from_slice(include_bytes!("../../docs/signing/publisher.json")).map_err(|_|fail("Invalid public identity."))?;
    if sha(include_bytes!("../../docs/signing/publisher-key.der"))!="31ccb588ae7e529535dcccd94f06d8d39ae1b8847b7003e884ad8f44cd340c80"
        ||sha(include_bytes!("../../docs/signing/publisher-cert.der"))!=CERT_PIN||identity["publisher"]!="KiaroSama"||identity["algorithm"]!="rsa-pkcs1-sha256"
        ||identity["public_key_sha256"]!="31ccb588ae7e529535dcccd94f06d8d39ae1b8847b7003e884ad8f44cd340c80"||identity["certificate_sha256"]!=CERT_PIN{
        return Err(fail("Approved signing identity differs."));
    }
    let date=|key:&str|identity[key].as_str().and_then(|s|chrono::DateTime::parse_from_rfc3339(s).ok()).ok_or_else(||fail("Invalid signer validity."));
    let now=chrono::Utc::now();if now<date("not_before")?||now>date("not_after")?{return Err(fail("Signer outside validity."));}Ok(identity)
}
pub fn validate_password(bytes:&[u8])->io::Result<()>{
    if bytes.is_empty()||bytes.len()>4095||!bytes.iter().all(|b|b.is_ascii_alphanumeric()||b"+/=".contains(b)){Err(fail("Malformed selected signing password."))}else{Ok(())}
}
pub fn validate_authority(env:&Value)->io::Result<()>{
    if env["GITHUB_ACTIONS"]!="true"||env["GITHUB_REPOSITORY"]!="KiaroSama/Numera-Gen-MCP"||env["GITHUB_REF"]!="refs/heads/main"||env["GITHUB_EVENT_NAME"]!="workflow_dispatch"
        ||env["GITHUB_JOB"]!="signing"||env["NUMERA_CI_OPERATION"]!="qualify"{
        Err(fail("Protected hosted signing route not authorized."))
    }else{Ok(())}
}
fn tool(name:&str)->io::Result<PathBuf>{
    let path=std::env::var_os(format!("NUMERA_{}",name.to_uppercase())).map(PathBuf::from).ok_or_else(||fail("Qualified signing tool missing."))?;
    if !path.is_absolute()||!path.is_file(){return Err(fail("Qualified signing tool unavailable."));}path.canonicalize()
}
pub fn codesign_failure(output: &[u8]) -> io::Error {
    let diagnostic = output.to_ascii_lowercase();
    let contains = |text: &str| diagnostic.windows(text.len()).any(|window| window == text.as_bytes());
    for (reason, code) in [
        ("capability_unavailable", "macos_signing_capability_unavailable"),
        ("allocation_failed", "macos_signing_allocation_failed"),
        ("native_operation_failed", "macos_signing_native_operation_failed"),
        ("single_identity_required", "macos_signing_single_identity_required"),
        ("identity_not_found", "macos_signing_identity_not_found"),
        ("certificate_bound", "macos_signing_certificate_bound"),
        ("certificate_mismatch", "macos_signing_certificate_mismatch"),
        ("native_signature_failed", "macos_signing_native_signature_failed"),
    ] {
        if contains(&format!("\"code\": \"macos_signing_{reason}\"")) { return fail(code); }
    }
    for (messages, code) in [
        (&["no identity found", "specified item could not be found in the keychain"][..], "macos_signing_identity_not_found"),
        (&["unable to build chain to self-signed root"][..], "macos_signing_chain_unavailable"),
        (&["user interaction is not allowed", "interaction with the security server is not allowed"][..], "macos_signing_interaction_required"),
        (&["errsecinternalcomponent"][..], "macos_signing_internal_component"),
        (&["resource fork, finder information, or similar detritus not allowed"][..], "macos_signing_extended_attributes"),
    ] {
        if messages.iter().any(|message| contains(message)) { return fail(code); }
    }
    fail("macos_signing_rejected")
}

pub fn private_command(context:&Context,program:&Path)->Command{
    let mut command=context.command(program);command.env_clear();
    for name in ["PATH","SystemRoot","WINDIR","COMSPEC","PATHEXT","HOME","USERPROFILE","TMP","TEMP","TMPDIR"]{
        if let Some(value)=std::env::var_os(name){command.env(name,value);}
    }command
}
fn openssl(context:&Context,args:&[&str])->io::Result<Vec<u8>>{context.run(private_command(context,&tool("openssl")?).args(args),30000,15000,true)}
fn text(path:&Path)->io::Result<&str>{path.to_str().ok_or_else(||fail("Signing path must be Unicode."))}
#[cfg(target_os="macos")]
fn keychain_state(context:&Context)->io::Result<[Vec<u8>;2]>{
    Ok([context.capture(private_command(context,Path::new("/usr/bin/security")).args(["list-keychains","-d","user"]),30000,15000,1024*1024)?,
        context.capture(private_command(context,Path::new("/usr/bin/security")).args(["default-keychain","-d","user"]),30000,15000,1024*1024)?])
}

pub fn sign(context:&Context,target:&str)->io::Result<()> {
    context.hosted()?;distribution::target(target)?;public_identity()?;
    let authority=json!({"GITHUB_ACTIONS":std::env::var("GITHUB_ACTIONS").ok(),"GITHUB_REPOSITORY":std::env::var("GITHUB_REPOSITORY").ok(),"GITHUB_REF":std::env::var("GITHUB_REF").ok(),"GITHUB_EVENT_NAME":std::env::var("GITHUB_EVENT_NAME").ok(),"GITHUB_JOB":std::env::var("GITHUB_JOB").ok(),"NUMERA_CI_OPERATION":std::env::var("NUMERA_CI_OPERATION").ok()});
    validate_authority(&authority)?;
    let encoded=Secret(std::env::var("NUMERA_SIGNING_PFX_BASE64").map_err(|_|fail("Selected identity missing."))?.into_bytes());
    let pfx=Secret(STANDARD.decode(&encoded.0).map_err(|_|fail("Malformed selected identity."))?);
    let password=Secret(std::env::var("NUMERA_SIGNING_PASSWORD").map_err(|_|fail("Selected password missing."))?.into_bytes());validate_password(&password.0)?;
    let prepared=context.relative(&format!(".ci-work/prepared/{target}"))?;
    let inventory=distribution::read_tree(&prepared,target,&context.source_sha()?)?;
    if inventory.contains_key("manifest.sig"){return Err(fail("Signing candidate already signed."));}
    let inputs=PrivateInputs::create(&context.root,&pfx.0,&password.0)?;
    let operation=(||{
        let mut manifest:Value=serde_json::from_slice(&inventory["manifest.json"]).map_err(|_|fail("Invalid signing candidate."))?;
        let pass=format!("file:{}",text(&inputs.password)?);
        let cert=openssl(context,&["pkcs12","-in",text(&inputs.pfx)?,"-clcerts","-nokeys","-passin",&pass])?;
        let cert=std::str::from_utf8(&cert).map_err(|_|fail("Invalid certificate encoding."))?;
        let (_,tail)=cert.split_once("-----BEGIN CERTIFICATE-----").ok_or_else(||fail("Selected certificate missing."))?;
        let (body,rest)=tail.split_once("-----END CERTIFICATE-----").ok_or_else(||fail("Selected certificate malformed."))?;
        if rest.contains("-----BEGIN CERTIFICATE-----")||sha(&STANDARD.decode(body.split_whitespace().collect::<String>()).map_err(|_|fail("Selected certificate malformed."))?)!=CERT_PIN{return Err(fail("Selected PFX certificate pin differs."));}
        let suffix=if target.contains("windows"){".exe"}else{""};let files=[prepared.join(format!("bin/numera-gen-mcp{suffix}")),prepared.join(format!("bin/numera-dev{suffix}"))];
        if target.contains("windows"){
            for file in &files{
                let signed=file.with_file_name(format!("{}.signed-{}",file.file_name().ok_or_else(||fail("Signing filename missing."))?.to_string_lossy(),uuid::Uuid::new_v4()));let signer=tool("osslsigncode")?;
                let result=context.run(private_command(context,&signer).args(["sign","-pkcs12",text(&inputs.pfx)?,"-readpass",text(&inputs.password)?,"-h","sha256","-n","Numera Gen MCP","-in",text(file)?,"-out",text(&signed)?]),30000,15000,true).and_then(|_|fs::rename(&signed,file));
                if signed.try_exists()?{fs::remove_file(&signed)?;}
                result?;
            }
        }else if target.contains("apple"){
            #[cfg(target_os="macos")] {
                let before=keychain_state(context)?;
                let result=crate::signing_macos::sign(&pfx.0,&password.0,CERT_PIN,&files.iter().map(PathBuf::as_path).collect::<Vec<_>>());
                if keychain_state(context)?!=before{return Err(fail("Signing changed keychain preferences."));}
                result?;
            }
            #[cfg(not(target_os="macos"))]return Err(fail("Native macOS signer required."));
        }
        for row in manifest["files"].as_array_mut().ok_or_else(||fail("Signing inventory missing."))?{
            let name=row["path"].as_str().ok_or_else(||fail("Invalid signing member."))?;let bytes=artifact::read(&prepared.canonicalize()?,name,256*1024*1024).map_err(|_|fail("Signing member changed."))?;
            row["bytes"]=json!(bytes.len());row["sha256"]=json!(sha(&bytes));
        }
        manifest["binary_sha256"]=json!(sha(&read_regular(&files[0],256*1024*1024)?));
        let mut bytes=serde_json::to_vec_pretty(&manifest).map_err(|_|fail("Cannot encode signed manifest."))?;bytes.push(b'\n');fs::write(prepared.join("manifest.json"),bytes)?;
        openssl(context,&["dgst","-sha256","-keyform","P12","-sign",text(&inputs.pfx)?,"-passin",&pass,"-out",text(&prepared.join("manifest.sig"))?,text(&prepared.join("manifest.json"))?])?;
        artifact::verify_activation(&prepared).map_err(|_|fail("Signed candidate rejected."))?;Ok(())
    })();
    let cleanup=inputs.cleanup();operation?;cleanup?;
    println!("{}",json!({"component":"native-signing","target":target,"source_sha":context.source_sha()?,"publisher_inputs_disposed":true,"public_os_trust":false}));Ok(())
}

pub fn verify(context:&Context,target:&str)->io::Result<()> {
    let root=context.relative(&format!(".ci-work/prepared/{target}"))?;
    let prepared=artifact::verify_activation(&root).map_err(|_|fail("Native prepared signature rejected."))?;
    if prepared.manifest.source_sha!=context.source_sha()?{return Err(fail("Signed source differs."));}
    let entries=distribution::read_tree(&root,target,&context.source_sha()?)?;
    distribution::archive(context,&entries,target,false)
}
