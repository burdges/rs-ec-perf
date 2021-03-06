use super::*;

use reed_solomon_erasure::galois_16::ReedSolomon;

pub fn to_shards(payload: &[u8]) -> Vec<WrappedShard> {
	let base_len = payload.len();

	// how many bytes we actually need.
	let needed_shard_len = (base_len + DATA_SHARDS - 1) / DATA_SHARDS;

	// round up, ing GF(2^16) there are only 2 byte values, so each shard must a multiple of 2
	let needed_shard_len = needed_shard_len + (needed_shard_len & 0x01);

	let shard_len = needed_shard_len;

	let mut shards = vec![WrappedShard::new(vec![0u8; shard_len]); N_VALIDATORS];
	for (data_chunk, blank_shard) in payload.chunks(shard_len).zip(&mut shards) {
		// fill the empty shards with the corresponding piece of the payload,
		// zero-padded to fit in the shards.
		let len = std::cmp::min(shard_len, data_chunk.len());
		let blank_shard: &mut [u8] = blank_shard.as_mut();
		blank_shard[..len].copy_from_slice(&data_chunk[..len]);
	}

	shards
}

pub fn rs() -> ReedSolomon {
	ReedSolomon::new(DATA_SHARDS, PARITY_SHARDS).expect("this struct is not created with invalid shard number; qed")
}

pub fn encode(data: &[u8]) -> Vec<WrappedShard> {
	let encoder = rs();
	let mut shards = to_shards(data);
	encoder.encode(&mut shards).unwrap();
	shards
}

pub fn reconstruct(mut received_shards: Vec<Option<WrappedShard>>) -> Option<Vec<u8>> {
	let r = rs();

	// Try to reconstruct missing shards
	r.reconstruct_data(&mut received_shards).expect("Sufficient shards must be received. qed");

	// Convert back to normal shard arrangement
	// let l = received_shards.len();

	// let result_data_shards= received_shards
	// 	.into_iter()
	// 	.filter_map(|x| x)
	// 	.collect::<Vec<WrappedShard>>();

	let result = received_shards.into_iter().filter_map(|x| x).take(DATA_SHARDS).fold(
		Vec::with_capacity(12 << 20),
		|mut acc, x| {
			acc.extend_from_slice(x.into_inner().as_slice());
			acc
		},
	);

	Some(result)
}
