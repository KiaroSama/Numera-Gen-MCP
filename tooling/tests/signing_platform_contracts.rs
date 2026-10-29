use numera_task::{private_files::PrivateInputs,signing::validate_password};
use std::{fs,path::{Path,PathBuf}};

// Compile the same portable ABI choreography on every test target; the Darwin adapter remains
// framework-only. No Apple framework, keychain, certificate or signing operation runs in these tests.
mod context { pub use numera_task::context::{fail,sha}; }
#[path = "../src/apple_signer.rs"]
mod apple_signer;

struct Fixture(PathBuf);
impl Fixture{fn new()->Self{let root=Path::new(env!("CARGO_MANIFEST_DIR")).join("../.ci-work/tooling-tests").join(format!("private signing {}",uuid::Uuid::new_v4()));fs::create_dir_all(&root).unwrap();Self(root)}}
impl Drop for Fixture{fn drop(&mut self){if let Err(error)=fs::remove_dir_all(&self.0){if std::thread::panicking(){eprintln!("Private signing fixture cleanup failed: {:?}",error.kind());}else{panic!("Private signing fixture cleanup failed: {:?}",error.kind());}}}}

#[test]
fn selected_input_password_bound_includes_one_transport_newline_without_storing_it(){
    let fixture=Fixture::new();let password=vec![b'x';4095];validate_password(&password).unwrap();
    let inputs=PrivateInputs::create(&fixture.0,b"synthetic encrypted PFX; never imported",&password).unwrap();
    assert_eq!(fs::read(&inputs.password).unwrap(),password);assert_eq!(password.len()+b"\n".len(),4096);
    assert_eq!(fs::read(&inputs.pfx).unwrap(),b"synthetic encrypted PFX; never imported");
    let directory=inputs.directory.clone();inputs.cleanup().unwrap();assert!(!directory.exists());
    let too_long=vec![b'x';4096];assert!(validate_password(&too_long).is_err());assert!(PrivateInputs::create(&fixture.0,b"synthetic",&too_long).is_err());assert!(!directory.exists());
}

#[test]
fn temporary_signing_inputs_are_exclusive_and_normal_or_unwind_cleanup_removes_only_owned_files(){
    let fixture=Fixture::new();let unrelated=fixture.0.join("retained.txt");fs::write(&unrelated,b"retain exact unrelated bytes").unwrap();
    let inputs=PrivateInputs::create(&fixture.0,b"synthetic encrypted PFX",b"fixturepassword").unwrap();let directory=inputs.directory.clone();
    assert!(PrivateInputs::create(&fixture.0,b"replacement",b"otherpassword").is_err());
    assert_eq!(fs::read(&inputs.pfx).unwrap(),b"synthetic encrypted PFX");assert_eq!(fs::read(&inputs.password).unwrap(),b"fixturepassword");
    #[cfg(unix)]{use std::os::unix::fs::PermissionsExt;assert_eq!(fs::metadata(&directory).unwrap().permissions().mode()&0o777,0o700);for path in[&inputs.pfx,&inputs.password]{assert_eq!(fs::metadata(path).unwrap().permissions().mode()&0o777,0o600);}}
    drop(inputs);assert!(!directory.exists());assert_eq!(fs::read(&unrelated).unwrap(),b"retain exact unrelated bytes");
    let unwind=std::panic::catch_unwind(||{let _inputs=PrivateInputs::create(&fixture.0,b"synthetic second PFX",b"fixturepassword").unwrap();panic!("Controlled fixture unwind");});
    assert!(unwind.is_err());assert!(!directory.exists());assert_eq!(fs::read(unrelated).unwrap(),b"retain exact unrelated bytes");
}

#[test]
fn invalid_temporary_material_is_rejected_before_any_private_staging_file_exists(){
    let fixture=Fixture::new();let path=fixture.0.join(".ci-work/signing");
    for(pfx,password)in[(Vec::new(),b"fixture".to_vec()),(vec![0;32769],b"fixture".to_vec()),(b"fixture".to_vec(),Vec::new()),(b"fixture".to_vec(),b"private\nvalue".to_vec()),(b"fixture".to_vec(),b"private value".to_vec()),(b"fixture".to_vec(),vec![b'x';4096])]{assert!(PrivateInputs::create(&fixture.0,&pfx,&password).is_err());assert!(!path.exists());}
}

mod apple_abi {
    use super::apple_signer::{Api,Ref,sign_with};
    use std::{cell::RefCell,collections::{BTreeMap,BTreeSet},ffi::c_void,io,path::Path,ptr};
    const CERTIFICATE:&[u8]=b"synthetic public certificate; not a native signature";
    const CONSTANTS:[&str;7]=["kSecImportToMemoryOnly","kSecImportExportPassphrase","kSecImportItemIdentity","kSecCodeSignerIdentity","kSecCodeSignerRequireTimestamp","kCFBooleanTrue","kCFBooleanFalse"];
    #[derive(Clone,Copy)]
    enum Failure { None, WrongCertificate, Multiple, Import, Signature, Missing(&'static str), NullConstant(&'static str), NullOutput(&'static str), ErrorOutput(&'static str) }
    enum Object { Bytes(Vec<u8>), Dictionary(BTreeMap<usize,usize>), Other }
    struct Fake {
        next:usize, objects:BTreeMap<usize,Object>, owned:Vec<usize>, released:Vec<usize>,
        calls:Vec<&'static str>, resolved:Vec<String>, parameters:Option<BTreeMap<usize,usize>>, failure:Failure,
        slots:[usize;7], certificate:Vec<u8>,
    }
    impl Fake {
        fn new(failure:Failure)->Self {Self {next:100,objects:BTreeMap::new(),owned:Vec::new(),released:Vec::new(),calls:Vec::new(),resolved:Vec::new(),parameters:None,failure,slots:std::array::from_fn(|n|1000+n),certificate:if matches!(failure,Failure::WrongCertificate){b"unrelated signer".to_vec()}else{CERTIFICATE.to_vec()}}}
        fn own(&mut self,object:Object)->Ref {self.next+=1;self.objects.insert(self.next,object);self.owned.push(self.next);self.next as Ref}
        fn allocation(&mut self,name:&'static str,object:Object)->Ref {self.calls.push(name);if matches!(self.failure,Failure::NullOutput(target) if target==name){ptr::null()}else{self.own(object)}}
        fn output(&mut self,name:&'static str,out:*mut Ref)->i32 {
            self.calls.push(name);
            if matches!(self.failure,Failure::NullOutput(target) if target==name){unsafe{*out=ptr::null()};return 0;}
            unsafe{*out=self.own(Object::Other)};
            if matches!(self.failure,Failure::ErrorOutput(target) if target==name){-50}else{0}
        }
        fn dictionary(&self,value:Ref)->BTreeMap<usize,usize>{match self.objects.get(&(value as usize)).unwrap(){Object::Dictionary(value)=>value.clone(),_=>panic!("expected fake dictionary")}}
        fn assert_released(&self){let mut reverse=self.owned.clone();reverse.reverse();assert_eq!(self.released,reverse,"CF owners release in reverse lifetime order");let mut owned=self.owned.clone();let mut released=self.released.clone();owned.sort_unstable();released.sort_unstable();assert_eq!(owned,released,"every created/copied/output object must be released");assert_eq!(self.released.len(),self.released.iter().collect::<BTreeSet<_>>().len(),"no double release");assert!(self.calls.iter().all(|name|!name.contains("Keychain")&&!name.contains("Trust")&&!name.contains("Export")));}
        fn live(&self,value:Ref){assert!(self.objects.contains_key(&(value as usize)),"reference was never owned");assert!(!self.released.contains(&(value as usize)),"reference already released");}
    }
    thread_local!{static ACTIVE:RefCell<Fake>=RefCell::new(Fake::new(Failure::None));}
    fn with<T>(action:impl FnOnce(&mut Fake)->T)->T{ACTIVE.with(|state|action(&mut state.borrow_mut()))}
    fn symbol(name:&[u8])->io::Result<*mut c_void>{
        let name=std::str::from_utf8(name.strip_suffix(&[0]).unwrap()).unwrap();
        with(|state|{
            state.resolved.push(name.to_owned());
            if matches!(state.failure,Failure::Missing(missing) if missing==name){return Err(crate::context::fail("macos_signing_capability_unavailable"));}
            if let Some(index)=CONSTANTS.iter().position(|item|*item==name){if matches!(state.failure,Failure::NullConstant(missing) if missing==name){state.slots[index]=0;}return Ok((&mut state.slots[index] as *mut usize).cast());}
            let address=match name {
                "CFDataCreate"=>data as *const (),"CFStringCreateWithBytes"=>string as *const (),"CFDictionaryCreate"=>dictionary as *const (),
                "CFArrayGetCount"=>count as *const (),"CFArrayGetValueAtIndex"=>item as *const (),"CFDictionaryGetValue"=>identity as *const (),
                "CFDataGetLength"=>length as *const (),"CFDataGetBytePtr"=>data_pointer as *const (),"CFURLCreateFromFileSystemRepresentation"=>url as *const (),
                "CFRelease"=>release as *const (),"SecPKCS12Import"=>import as *const (),"SecIdentityCopyCertificate"=>certificate as *const (),
                "SecCertificateCopyData"=>certificate_data as *const (),"SecStaticCodeCreateWithPath"=>code as *const (),"SecCodeSignerCreate"=>signer as *const (),
                "SecCodeSignerAddSignatureWithErrors"=>add as *const (),_=>panic!("unexpected native symbol {name}"),
            };Ok(address as *mut c_void)
        })
    }
    unsafe extern "C" fn data(_:Ref,bytes:*const u8,size:isize)->Ref{with(|s|s.allocation("CFDataCreate",Object::Bytes(unsafe{std::slice::from_raw_parts(bytes,size as usize)}.to_vec())))}
    unsafe extern "C" fn string(_:Ref,bytes:*const u8,size:isize,encoding:u32,external:u8)->Ref{assert_eq!(encoding,0x08000100);assert_eq!(external,0);with(|s|s.allocation("CFStringCreateWithBytes",Object::Bytes(unsafe{std::slice::from_raw_parts(bytes,size as usize)}.to_vec())))}
    unsafe extern "C" fn dictionary(_:Ref,keys:*const Ref,values:*const Ref,size:isize,key_callbacks:Ref,value_callbacks:Ref)->Ref{assert!(key_callbacks.is_null()&&value_callbacks.is_null());let pairs=(0..size as usize).map(|n|unsafe{(*keys.add(n)as usize,*values.add(n)as usize)}).collect();with(|s|s.allocation("CFDictionaryCreate",Object::Dictionary(pairs)))}
    unsafe extern "C" fn count(_:Ref)->isize{with(|s|{s.calls.push("CFArrayGetCount");if matches!(s.failure,Failure::Multiple){2}else{1}})}
    unsafe extern "C" fn item(_:Ref,index:isize)->Ref{assert_eq!(index,0);with(|s|s.calls.push("CFArrayGetValueAtIndex"));4000usize as Ref}
    unsafe extern "C" fn identity(_:Ref,key:Ref)->Ref{assert_eq!(key as usize,1002);with(|s|{s.calls.push("CFDictionaryGetValue");if matches!(s.failure,Failure::NullOutput("identity")){ptr::null()}else{4001usize as Ref}})}
    unsafe extern "C" fn length(_:Ref)->isize{with(|s|{s.calls.push("CFDataGetLength");s.certificate.len() as isize})}
    unsafe extern "C" fn data_pointer(_:Ref)->*const u8{with(|s|{s.calls.push("CFDataGetBytePtr");if matches!(s.failure,Failure::NullOutput("certificate-bytes")){ptr::null()}else{s.certificate.as_ptr()}})}
    unsafe extern "C" fn url(_:Ref,bytes:*const u8,size:isize,directory:u8)->Ref{assert_eq!(directory,0);assert!(size>0);let _=unsafe{std::slice::from_raw_parts(bytes,size as usize)};with(|s|s.allocation("CFURLCreateFromFileSystemRepresentation",Object::Other))}
    unsafe extern "C" fn release(value:Ref){with(|s|{s.calls.push("CFRelease");assert!(s.objects.contains_key(&(value as usize)));s.released.push(value as usize);});}
    unsafe extern "C" fn import(data:Ref,options:Ref,out:*mut Ref)->i32{with(|s|{
        assert_eq!(s.resolved.len(),23,"qualify every16functions+7constants before secret import");
        s.live(data);s.live(options);let options=s.dictionary(options);assert_eq!(options.len(),2);assert_eq!(options[&1000],1005);
        let pass=options[&1001]as Ref;s.live(pass);
        match s.objects.get(&(pass as usize)).unwrap(){Object::Bytes(bytes)=>assert_eq!(bytes,b"fixturepassword"),_=>panic!("password missing")}
        match s.objects.get(&(data as usize)).unwrap(){Object::Bytes(bytes)=>assert_eq!(bytes,b"synthetic encrypted PFX"),_=>panic!("PFX bytes missing")}
        let status=s.output("SecPKCS12Import",out);if matches!(s.failure,Failure::Import){-25293}else{status}
    })}
    unsafe extern "C" fn certificate(identity:Ref,out:*mut Ref)->i32{assert_eq!(identity as usize,4001);with(|s|s.output("SecIdentityCopyCertificate",out))}
    unsafe extern "C" fn certificate_data(_:Ref)->Ref{with(|s|{let bytes=s.certificate.clone();s.allocation("SecCertificateCopyData",Object::Bytes(bytes))})}
    unsafe extern "C" fn code(_:Ref,flags:u32,out:*mut Ref)->i32{assert_eq!(flags,0);with(|s|s.output("SecStaticCodeCreateWithPath",out))}
    unsafe extern "C" fn signer(parameters:Ref,flags:u32,out:*mut Ref)->i32{assert_eq!(flags,0);with(|s|{s.live(parameters);s.parameters=Some(s.dictionary(parameters));s.output("SecCodeSignerCreate",out)})}
    unsafe extern "C" fn add(signer:Ref,code:Ref,flags:u32,error:*mut Ref)->i32{assert_eq!(flags,0);with(|s|{s.live(signer);s.live(code);for value in s.owned.iter().take(4){s.live(*value as Ref);}assert!(s.owned.iter().filter(|id|!s.released.contains(id)).count()>=9);s.calls.push("SecCodeSignerAddSignatureWithErrors");if matches!(s.failure,Failure::Signature){unsafe{*error=s.own(Object::Bytes(b"private error details; never rendered".to_vec()))};-50}else{0}})}
    fn run(failure:Failure)->io::Result<()>{
        with(|state|*state=Fake::new(failure));
        let pin=crate::context::sha(CERTIFICATE);
        // SAFETY: the test resolver supplies exact ABI-compatible function pointers and stable
        // thread-local constant slots; all fake objects remain live through their release calls.
        unsafe{sign_with(symbol,b"synthetic encrypted PFX",b"fixturepassword",&pin,&[Path::new("synthetic-application")])}
    }

    #[test]
    fn memory_only_identity_is_pinned_and_all_native_references_are_released(){
        for failure in[Failure::None,Failure::WrongCertificate,Failure::Multiple,Failure::Import,Failure::Signature]{
            let result=run(failure);with(|state|{
                if matches!(failure,Failure::None){result.unwrap();assert_eq!(state.parameters,Some(BTreeMap::from([(1003,4001),(1004,1006)])));}
                else{let error=result.unwrap_err().to_string();assert!(!error.contains("private error details"));if !matches!(failure,Failure::Signature){assert!(!state.calls.contains(&"SecCodeSignerAddSignatureWithErrors"));}}
                state.assert_released();
            });
        }
    }
    #[test]
    fn missing_memory_only_symbol_fails_before_private_import_without_fallback(){
        let error=run(Failure::Missing("kSecImportToMemoryOnly")).unwrap_err();assert_eq!(error.to_string(),"macos_signing_capability_unavailable");
        with(|state|{assert!(state.calls.is_empty());state.assert_released();});
        for name in["CFDataCreate","CFStringCreateWithBytes","CFDictionaryCreate","CFArrayGetCount","CFArrayGetValueAtIndex","CFDictionaryGetValue","CFDataGetLength","CFDataGetBytePtr","CFURLCreateFromFileSystemRepresentation","CFRelease","SecPKCS12Import","SecIdentityCopyCertificate","SecCertificateCopyData","SecStaticCodeCreateWithPath","SecCodeSignerCreate","SecCodeSignerAddSignatureWithErrors"].into_iter().chain(CONSTANTS){
            let error=run(Failure::Missing(name)).unwrap_err();assert_eq!(error.to_string(),"macos_signing_capability_unavailable");with(|state|{assert!(state.calls.is_empty());state.assert_released();});
        }
        for name in CONSTANTS{let error=run(Failure::NullConstant(name)).unwrap_err();assert_eq!(error.to_string(),"macos_signing_capability_unavailable");with(|state|{assert!(state.calls.is_empty());state.assert_released();});}
    }
    #[test]
    fn every_allocation_and_partial_failure_output_is_released_once(){
        for name in["CFDataCreate","CFStringCreateWithBytes","CFDictionaryCreate","SecPKCS12Import","SecIdentityCopyCertificate","SecCertificateCopyData","CFURLCreateFromFileSystemRepresentation","SecStaticCodeCreateWithPath","SecCodeSignerCreate","identity","certificate-bytes"]{
            assert!(run(Failure::NullOutput(name)).is_err(),"null {name}");with(|s|s.assert_released());
        }
        for name in["SecPKCS12Import","SecIdentityCopyCertificate","SecStaticCodeCreateWithPath","SecCodeSignerCreate"]{assert!(run(Failure::ErrorOutput(name)).is_err(),"partial {name}");with(|s|s.assert_released());}
    }
    #[test]
    fn invalid_memory_input_is_rejected_before_symbol_resolution_or_secret_import(){
        let pin=crate::context::sha(CERTIFICATE);let path=Path::new("synthetic-application");
        for(pfx,password,pin,files)in[(Vec::new(),b"fixture".to_vec(),pin.clone(),vec![path]),(vec![0;1048577],b"fixture".to_vec(),pin.clone(),vec![path]),(b"fixture".to_vec(),Vec::new(),pin.clone(),vec![path]),(b"fixture".to_vec(),vec![b'x';4096],pin.clone(),vec![path]),(b"fixture".to_vec(),b"bad\npassword".to_vec(),pin.clone(),vec![path]),(b"fixture".to_vec(),b"fixture".to_vec(),"A".repeat(64),vec![path]),(b"fixture".to_vec(),b"fixture".to_vec(),"0".repeat(63),vec![path]),(b"fixture".to_vec(),b"fixture".to_vec(),pin.clone(),vec![]),(b"fixture".to_vec(),b"fixture".to_vec(),pin.clone(),vec![path,path,path])]{
            with(|s|*s=Fake::new(Failure::None));assert!(unsafe{sign_with(symbol,&pfx,&password,&pin,&files)}.is_err());with(|s|{assert!(s.resolved.is_empty());assert!(s.calls.is_empty());});
        }
    }
    #[test]
    fn qualification_alone_resolves_capabilities_without_importing_or_allocating(){
        with(|s|*s=Fake::new(Failure::None));unsafe{Api::resolve(symbol)}.unwrap();with(|s|{assert_eq!(s.resolved.len(),23);assert!(s.calls.is_empty());});
    }
}
