use crate::{types::block_identifier::BlockIdentifier, RpcErr, RpcHandler};
use ethereum_rust_core::{H160, U256};
use ethereum_rust_storage::Store;
use serde_json::{from_value, Value};

pub struct LogsRequest {
    /// The oldest block from which to start
    /// retrieving logs.
    /// Will default to `latest` if not provided.
    pub from: BlockIdentifier,
    /// Up to which block to stop retrieving logs.
    /// Will default to `latest` if not provided.
    pub to: BlockIdentifier,
    /// The addresses from where the logs origin from.
    pub address: Option<Vec<H160>>,
    /// Which topics to filter.
    pub topics: Option<Vec<U256>>,
}
impl RpcHandler for LogsRequest {
    fn parse(params: &Option<Vec<Value>>) -> Result<LogsRequest, RpcErr> {
        match params.as_deref() {
            Some([from, to, address, topics]) => Ok(LogsRequest {
                from: BlockIdentifier::parse(from.clone(), 0).unwrap(),
                to: BlockIdentifier::parse(to.clone(), 1).unwrap(),
                address: from_value(address.clone()).ok(),
                topics: from_value(topics.clone()).ok(),
            }),
            _ => Err(RpcErr::BadParams),
        }
    }
    fn handle(&self, storage: Store) -> Result<Value, RpcErr> {
        let Ok(Some(from)) = self.from.resolve_block_number(&storage) else {
            return Err(RpcErr::BadParams);
        };
        let Ok(Some(to)) = self.to.resolve_block_number(&storage) else {
            return Err(RpcErr::BadParams);
        };
        let logs = storage
            .get_logs_in_range(to, from)
            .map_err(|_| RpcErr::Internal)?;
        serde_json::to_value(logs).map_err(|_| RpcErr::Internal)
    }
}
