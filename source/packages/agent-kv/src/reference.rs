use std::{fmt, sync::Arc};
use sha2::{Digest, Sha256};
use crate::typed_blob_store::{BlobCodec, TypedBlobStore};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlobRef(String);
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlobRefError { InvalidFormat, InvalidLength(usize), DigestMismatch { expected: String, actual: String } }
impl fmt::Display for BlobRefError {
 fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result { match self {
  Self::InvalidFormat=>write!(f,"blob reference must be exactly 64 hexadecimal characters"),
  Self::InvalidLength(n)=>write!(f,"SHA-256 digest must be 32 bytes, got {n}"),
  Self::DigestMismatch{expected,actual}=>write!(f,"blob digest mismatch: expected {expected}, computed {actual}")
 }}
}
impl std::error::Error for BlobRefError {}
impl BlobRef {
 pub fn parse(value:impl AsRef<str>)->Result<Self,BlobRefError>{let v=value.as_ref();if v.len()!=64||!v.bytes().all(|b|b.is_ascii_hexdigit()){return Err(BlobRefError::InvalidFormat)} Ok(Self(v.to_ascii_lowercase()))}
 pub fn from_content(bytes:&[u8])->Self{Self(hex(&Sha256::digest(bytes)))}
 pub fn from_digest_bytes(bytes:&[u8])->Result<Self,BlobRefError>{if bytes.len()!=32{return Err(BlobRefError::InvalidLength(bytes.len()))}Ok(Self(hex(bytes)))}
 pub fn as_str(&self)->&str{&self.0}
 pub fn to_bytes(&self)->[u8;32]{decode(&self.0).expect("validated BlobRef").try_into().expect("SHA-256 length")}
 pub fn verify(&self,bytes:&[u8])->Result<(),BlobRefError>{let actual=Self::from_content(bytes);if *self==actual{Ok(())}else{Err(BlobRefError::DigestMismatch{expected:self.0.clone(),actual:actual.0})}}
}
fn hex(bytes:&[u8])->String{const H:&[u8;16]=b"0123456789abcdef";let mut s=String::with_capacity(bytes.len()*2);for b in bytes{s.push(H[(b>>4)as usize]as char);s.push(H[(b&15)as usize]as char)}s}
fn decode(s:&str)->Result<Vec<u8>,BlobRefError>{if s.len()!=64||!s.bytes().all(|b|b.is_ascii_hexdigit()){return Err(BlobRefError::InvalidFormat)}let mut out=Vec::with_capacity(32);for p in s.as_bytes().chunks_exact(2){let h=(p[0]as char).to_digit(16).ok_or(BlobRefError::InvalidFormat)?;let l=(p[1]as char).to_digit(16).ok_or(BlobRefError::InvalidFormat)?;out.push(((h<<4)|l)as u8)}Ok(out)}

struct LastWrite<T>{value:Arc<T>,reference:BlobRef,bytes:Vec<u8>}
/// A cache/serialization helper only; canonical state remains with its caller.
pub struct EagerReference<T>{value:Arc<T>,last:Option<LastWrite<T>>}
impl<T>EagerReference<T>{
 pub fn new(value:T)->Self{Self{value:Arc::new(value),last:None}}
 pub fn get(&self)->Arc<T>{Arc::clone(&self.value)}
 pub fn set(&mut self,value:T){self.value=Arc::new(value)}
 pub fn write_to_blob_store<C>(&mut self,store:&mut TypedBlobStore<C>)->Result<BlobRef,String> where C:BlobCodec<T>{
  if let Some(last)=&self.last{if Arc::ptr_eq(&self.value,&last.value){if store.is_blob_durable(&last.reference)==Some(false){store.put_encoded_with_reference::<T>(&last.reference,&last.bytes,true)?}return Ok(last.reference.clone())}}
  let (reference,bytes)=store.put_value(self.value.as_ref())?;self.last=Some(LastWrite{value:Arc::clone(&self.value),reference:reference.clone(),bytes});Ok(reference)
 }
}
/// Lazy reference whose identifier is stable until a changed value is persisted.
pub struct LazyReference<T>{reference:BlobRef,value:Option<Arc<T>>,last:Option<Arc<T>>,last_bytes:Option<Vec<u8>>}
impl<T>LazyReference<T>{
 pub fn new(reference:BlobRef)->Self{Self{reference,value:None,last:None,last_bytes:None}}
 pub fn blob_ref(&self)->&BlobRef{&self.reference}
 pub fn set(&mut self,value:T){self.value=Some(Arc::new(value))}
 pub fn get<C>(&mut self,store:&TypedBlobStore<C>)->Result<Arc<T>,String> where C:BlobCodec<T>{
  if let Some(v)=&self.value{return Ok(Arc::clone(v))}let bytes=store.get_encoded(&self.reference)?;let v=Arc::new(store.decode_value(&bytes)?);self.last=Some(Arc::clone(&v));self.last_bytes=Some(bytes);self.value=Some(Arc::clone(&v));Ok(v)
 }
 pub fn write_to_blob_store<C>(&mut self,store:&mut TypedBlobStore<C>)->Result<BlobRef,String> where C:BlobCodec<T>{
  let Some(value)=self.value.as_ref().map(Arc::clone)else{return Ok(self.reference.clone())};
  if self.last.as_ref().is_some_and(|v|Arc::ptr_eq(v,&value)){if let Some(bytes)=&self.last_bytes{if store.is_blob_durable(&self.reference)==Some(false){store.put_encoded_with_reference::<T>(&self.reference,bytes,true)?}}return Ok(self.reference.clone())}
  let (reference,bytes)=store.put_value(value.as_ref())?;self.reference=reference.clone();self.last=Some(value);self.last_bytes=Some(bytes);Ok(reference)
 }
}
#[cfg(test)]mod tests{use super::*;use crate::{blob_store::BlobStore,serde::Utf8Codec};#[test]fn sha256_is_strict_and_verified(){let r=BlobRef::from_content(b"abc");assert_eq!(r.as_str(),"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");assert_eq!(BlobRef::parse(r.as_str().to_ascii_uppercase()).unwrap(),r);assert!(BlobRef::parse("bad").is_err());assert!(BlobRef::parse("g".repeat(64)).is_err());assert!(BlobRef::from_digest_bytes(&[0;31]).is_err());assert!(r.verify(b"no").is_err())}#[test]fn eager_lazy_roundtrip(){let mut s=TypedBlobStore::new(BlobStore::default(),Utf8Codec);let mut e=EagerReference::new("hello".to_string());let r=e.write_to_blob_store(&mut s).unwrap();let mut l=LazyReference::new(r.clone());assert_eq!(&*l.get(&s).unwrap(),"hello");assert_eq!(l.write_to_blob_store(&mut s).unwrap(),r);l.set("new".into());assert_ne!(l.write_to_blob_store(&mut s).unwrap(),r)}}
