#[path = "common/image.rs"]
mod image_fixture;
use numera_image_gen_mcp::{adapters::{build_request,types::Input},files::images::inspect,http::api::Body,services::requirements,jobs::Receipt};
use serde_json::{Value,json};
fn input<'a>(connection:&'a Value,request:&'a Value,references:&'a [numera_image_gen_mcp::files::images::Image],mask:Option<&'a numera_image_gen_mcp::files::images::Image>)->Input<'a>{Input{connection,request,references,mask,model:"opaque/image",operation:"edit"}}

#[test]
fn explicit_native_json_generation_fields_preserve_references_masks_and_reject_unconfigured_routes() {
    let image=inspect(image_fixture::original(),&json!({}),None).unwrap();let refs=vec![image.clone(),image.clone()];
    let request=json!({"prompt":"Preserve 日本語","count":1,"provider_options":{}});
    for mode in ["json","generation-json"] {
        let c=json!({"adapter":"openai-images","edit":{"mode":mode,"encoding":"images","maxReferences":2,"masks":true,"maskPolarity":"transparent-edit"}});
        let prepared=build_request(&input(&c,&request,&refs,Some(&image))).unwrap();assert_eq!(prepared.operation,if mode=="json"{"images/edits"}else{"images/generations"});
        let Body::Json(body)=prepared.body else{panic!("explicit JSON route required")};assert_eq!(body["images"].as_array().unwrap().len(),2);assert!(body["mask"].as_str().unwrap().starts_with("data:image/png;base64,"));
    }
    for (c,code,reference_count) in [
        (json!({"adapter":"openai-images"}),"unsupported_operation",1),
        (json!({"adapter":"openai-images","edit":{"mode":"json","encoding":"image","maxReferences":1}}),"unsupported_operation",2),
        (json!({"adapter":"openai-images","edit":{"mode":"json","encoding":"image[]","maxReferences":2,"masks":true,"maskPolarity":"transparent-edit"}}),"invalid_configuration",1),
        (json!({"adapter":"openai-images","edit":{"mode":"json","encoding":"image","maxReferences":1}}),"unsupported_operation",1),
    ] {assert_eq!(build_request(&input(&c,&request,&refs[..reference_count],Some(&image))).err().unwrap().code,code);}
    let c=json!({"adapter":"openai-images","gateway":"9router"});
    let r=json!({"prompt":"fixture","count":1,"quality":"high","provider_options":{}});
    let raw=Input {model:"antigravity/gemini-3.1-flash-image",operation:"generate",references:&[],mask:None,connection:&c,request:&r};
    assert_eq!(build_request(&raw).err().unwrap().code,"unsupported_parameter");
}

#[test]
fn requested_ratio_tier_format_and_upscale_deviations_use_original_dimensions_and_evidence() {
    let image=inspect(image_fixture::original(),&json!({}),None).unwrap();
    let receipt=Receipt::new(json!({"request_id":"fixture","output_requirements":{"output_format":"jpeg","size":"8x6","aspect_ratio":"1:1","image_size":"4K","upscale_source":{"width":7,"height":5},"dimension_evidence":{"pixel_sizes":[{"aspect_ratio":"1:1","resolution":"4K","width":4096,"height":4096}]}}}));
    let deviations=requirements::deviations(&receipt,&image,0);assert_eq!(deviations.len(),5);
    assert!(deviations.contains(&"Output 0 aspect ratio differs from requested aspect_ratio.".into()));assert!(deviations.contains(&"Output 0 dimensions differ from requested 4K tier.".into()));
    for ratio in ["7:5","not-ratio","0:1","1:0"] {
        let receipt=Receipt::new(json!({"output_requirements":{"aspect_ratio":ratio}}));assert!(requirements::deviations(&receipt,&image,0).is_empty());
    }
    let receipt=Receipt::new(json!({"output_requirements":{"aspect_ratio":"1:1","image_size":"1K","dimension_evidence":{"pixel_sizes":[{"aspect_ratio":"1:1","resolution":"1K","width":7,"height":5}]}}}));assert!(requirements::deviations(&receipt,&image,0).is_empty());
    assert!(requirements::deviations(&Receipt::new(json!({})),&image,0).is_empty());
}
