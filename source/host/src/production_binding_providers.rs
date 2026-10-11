#[derive(Clone,Debug,PartialEq,Eq)]
pub struct ProductionSecretsContext{pub machine_id:String,pub account_id:Option<String>}
pub fn production_secrets_context(machine_id:&str,account_id:Option<&str>)->Result<ProductionSecretsContext,&'static str>{
    if machine_id.trim().is_empty(){return Err("machine id is required");}
    Ok(ProductionSecretsContext{machine_id:machine_id.into(),account_id:account_id.map(str::to_string)})
}
