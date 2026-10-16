use std::marker::PhantomData;
use serde::{de::DeserializeOwned,Serialize};
use crate::typed_blob_store::BlobCodec;
#[derive(Clone,Copy,Debug,Default)]pub struct Utf8Codec;
impl BlobCodec<String> for Utf8Codec{fn encode(&self,v:&String)->Result<Vec<u8>,String>{Ok(v.as_bytes().to_vec())}fn decode(&self,b:&[u8])->Result<String,String>{String::from_utf8(b.to_vec()).map_err(|e|e.to_string())}fn blob_type(&self)->Option<&'static str>{Some("string")}}
pub struct JsonCodec<T>(PhantomData<fn()->T>);impl<T> JsonCodec<T>{pub fn new()->Self{Self(PhantomData)}}impl<T>Default for JsonCodec<T>{fn default()->Self{Self::new()}}
impl<T:Serialize+DeserializeOwned>BlobCodec<T> for JsonCodec<T>{fn encode(&self,v:&T)->Result<Vec<u8>,String>{serde_json::to_vec(v).map_err(|e|e.to_string())}fn decode(&self,b:&[u8])->Result<T,String>{serde_json::from_slice(b).map_err(|e|e.to_string())}fn blob_type(&self)->Option<&'static str>{Some("json")}}
