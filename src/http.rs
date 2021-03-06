use crate::{meta_ext::MetaExt, Error, StorageKey};
use async_trait::async_trait;
use frame_metadata::RuntimeMetadataPrefixed;
use futures_lite::prelude::*;
use jsonrpc::serde_json::{to_string, value::RawValue};
use std::{convert::TryInto, fmt};
pub use surf::Url;

#[derive(Debug)]
pub struct Backend(Url);

#[async_trait]
impl crate::Backend for Backend {
    async fn query_raw<K>(&self, key: K) -> crate::Result<Vec<u8>>
    where
        K: TryInto<StorageKey, Error = Error> + Send,
    {
        let key = key.try_into()?.to_string();
        log::debug!("StorageKey encoded: {}", key);
        self.rpc("state_getStorage", &[&key])
            .await
            // NOTE it could fail for more reasons
            .map_err(|_| Error::StorageKeyNotFound)
    }

    async fn submit<T>(&self, mut ext: T) -> crate::Result<()>
    where
        T: AsyncRead + Send + Unpin,
    {
        let mut extrinsic = vec![];
        ext.read_to_end(&mut extrinsic)
            .await
            .map_err(|_| Error::BadInput)?;
        let extrinsic = format!("0x{}", hex::encode(&extrinsic));
        log::debug!("Extrinsic: {}", extrinsic);

        let res = self
            .rpc("author_submitExtrinsic", &[&extrinsic])
            .await
            .map_err(|e| Error::Node(e.to_string()))?;
        log::debug!("Extrinsic {:x?}", res);
        Ok(())
    }

    async fn metadata(&self) -> crate::Result<RuntimeMetadataPrefixed> {
        let meta = self
            .rpc("state_getMetadata", &[])
            .await
            .map_err(|e| Error::Node(e.to_string()))?;
        let meta = RuntimeMetadataPrefixed::from_bytes(meta).map_err(|_| Error::BadMetadata);
        log::trace!("Metadata {:#?}", meta);
        meta
    }
}

impl Backend {
    pub fn new<U>(url: U) -> Self
    where
        U: TryInto<Url>,
        <U as TryInto<Url>>::Error: fmt::Debug,
    {
        Backend(url.try_into().expect("Url"))
    }

    /// HTTP based JSONRpc request expecting an hex encoded result
    async fn rpc(
        &self,
        method: &str,
        params: &[&str],
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        log::info!("RPC `{}` to {}", method, &self.0);
        let rpc_response: String = surf::post(&self.0)
            .content_type("application/json")
            .body(
                to_string(&jsonrpc::Request {
                    id: 1.into(),
                    jsonrpc: Some("2.0"),
                    method,
                    params: &params
                        .iter()
                        .map(|p| format!("\"{}\"", p))
                        .map(|p| RawValue::from_string(p.to_string()).unwrap())
                        .collect::<Vec<_>>(),
                })
                .unwrap(),
            )
            .await?
            .body_json::<jsonrpc::Response>()
            .await?
            .result()?;
        log::debug!("RPC Response: {}...", &rpc_response[..10]);
        // assume the response is a hex encoded string starting with "0x"
        let rpc_response = hex::decode(&rpc_response[2..])?;
        Ok(rpc_response)
    }
}
