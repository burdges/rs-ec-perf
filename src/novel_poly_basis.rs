// Encoding/erasure decoding for Reed-Solomon codes over binary extension fields
//
// Derived impl of `RSAErasureCode.c`.
//
// Lin, Han and Chung, "Novel Polynomial Basis and Its Application to Reed-Solomon Erasure Codes," FOCS14.
// (http://arxiv.org/abs/1404.3458)

#![allow(dead_code)]

use super::*;

use std::slice::from_raw_parts;

type GFSymbol = u16;

const FIELD_BITS: usize = 16;

const GENERATOR: GFSymbol = 0x2D; //x^16 + x^5 + x^3 + x^2 + 1

// Cantor basis
const BASE: [GFSymbol; FIELD_BITS] =
	[1_u16, 44234, 15374, 5694, 50562, 60718, 37196, 16402, 27800, 4312, 27250, 47360, 64952, 64308, 65336, 39198];

const FIELD_SIZE: usize = 1_usize << FIELD_BITS;

const MODULO: GFSymbol = (FIELD_SIZE - 1) as GFSymbol;

static mut LOG_TABLE: [GFSymbol; FIELD_SIZE] = [0_u16; FIELD_SIZE];
static mut EXP_TABLE: [GFSymbol; FIELD_SIZE] = [0_u16; FIELD_SIZE];

//-----Used in decoding procedure-------
//twisted factors used in FFT
static mut SKEW_FACTOR: [GFSymbol; MODULO as usize] = [0_u16; MODULO as usize];

//factors used in formal derivative
static mut B: [GFSymbol; FIELD_SIZE >> 1] = [0_u16; FIELD_SIZE >> 1];

//factors used in the evaluation of the error locator polynomial
static mut LOG_WALSH: [GFSymbol; FIELD_SIZE] = [0_u16; FIELD_SIZE];

//return a*EXP_TABLE[b] over GF(2^r)
fn mul_table(a: GFSymbol, b: GFSymbol) -> GFSymbol {
	if a != 0_u16 {
		unsafe {
			let offset = (LOG_TABLE[a as usize] as u32 + b as u32 & MODULO as u32)
				+ (LOG_TABLE[a as usize] as u32 + b as u32 >> FIELD_BITS);
			EXP_TABLE[offset as usize]
		}
	} else {
		0_u16
	}
}

const fn log2(mut x: usize) -> usize {
	let mut o: usize = 0;
	while x > 1 {
		x >>= 1;
		o += 1;
	}
	o
}

const fn is_power_of_2(x: usize) -> bool {
	return x > 0_usize && x & (x - 1) == 0;
}

//fast Walsh–Hadamard transform over modulo mod
fn walsh(data: &mut [GFSymbol], size: usize) {
	let mut depart_no = 1_usize;
	while depart_no < size {
		let mut j = 0;
		let depart_no_next = depart_no << 1;
		while j < size {
			for i in j..(depart_no + j) {
				let tmp2: u32 = data[i] as u32 + MODULO as u32 - data[i + depart_no] as u32;
				data[i] = ((data[i] as u32 + data[i + depart_no] as u32 & MODULO as u32)
					+ (data[i] as u32 + data[i + depart_no] as u32 >> FIELD_BITS)) as GFSymbol;
				data[i + depart_no] = ((tmp2 & MODULO as u32) + (tmp2 >> FIELD_BITS)) as GFSymbol;
			}
			j += depart_no_next;
		}
		depart_no = depart_no_next;
	}
}

//formal derivative of polynomial in the new basis
fn formal_derivative(cos: &mut [GFSymbol], size: usize) {
	for i in 1..size {
		let length = ((i ^ i - 1) + 1) >> 1;
		for j in (i - length)..i {
			cos[j] ^= cos.get(j + length).copied().unwrap_or_default();
		}
	}
	let mut i = size;
	while i < FIELD_SIZE && i < cos.len() {
		for j in 0..size {
			cos[j] ^= cos.get(j + i).copied().unwrap_or_default();
		}
		i <<= 1;
	}
}

//IFFT in the proposed basis
fn inverse_fft_in_novel_poly_basis(data: &mut [GFSymbol], size: usize, index: usize) {
	let mut depart_no = 1_usize;
	while depart_no < size {
		let mut j = depart_no;
		while j < size {
			for i in (j - depart_no)..j {
				data[i + depart_no] ^= data[i];
			}

			let skew = unsafe { SKEW_FACTOR[j + index - 1] };
			if skew != MODULO {
				for i in (j - depart_no)..j {
					data[i] ^= mul_table(data[i + depart_no], skew);
				}
			}

			j += depart_no << 1;
		}
		depart_no <<= 1;
	}
}

//FFT in the proposed basis
fn fft_in_novel_poly_basis(data: &mut [GFSymbol], size: usize, index: usize) {
	let mut depart_no = size >> 1_usize;
	while depart_no > 0 {
		let mut j = depart_no;
		while j < size {
			let skew = unsafe { SKEW_FACTOR[j + index - 1] };
			if skew != MODULO {
				for i in (j - depart_no)..j {
					data[i] ^= mul_table(data[i + depart_no], skew);
				}
			}
			for i in (j - depart_no)..j {
				data[i + depart_no] ^= data[i];
			}
			j += depart_no << 1;
		}
		depart_no >>= 1;
	}
}

//initialize LOG_TABLE[], EXP_TABLE[]
unsafe fn init() {
	let mas: GFSymbol = (1 << FIELD_BITS - 1) - 1;
	let mut state: usize = 1;
	for i in 0_usize..(MODULO as usize) {
		EXP_TABLE[state] = i as GFSymbol;
		if (state >> FIELD_BITS - 1) != 0 {
			state &= mas as usize;
			state = state << 1_usize ^ GENERATOR as usize;
		} else {
			state <<= 1;
		}
	}
	EXP_TABLE[0] = MODULO;

	LOG_TABLE[0] = 0;
	for i in 0..FIELD_BITS {
		for j in 0..(1 << i) {
			LOG_TABLE[j + (1 << i)] = LOG_TABLE[j] ^ BASE[i];
		}
	}
	for i in 0..FIELD_SIZE {
		LOG_TABLE[i] = EXP_TABLE[LOG_TABLE[i] as usize];
	}

	for i in 0..FIELD_SIZE {
		EXP_TABLE[LOG_TABLE[i] as usize] = i as GFSymbol;
	}
	EXP_TABLE[MODULO as usize] = EXP_TABLE[0];
}

//initialize SKEW_FACTOR[], B[], LOG_WALSH[]
unsafe fn init_dec() {
	let mut base: [GFSymbol; FIELD_BITS - 1] = Default::default();

	for i in 1..FIELD_BITS {
		base[i - 1] = 1 << i;
	}

	for m in 0..(FIELD_BITS - 1) {
		let step = 1 << (m + 1);
		SKEW_FACTOR[(1 << m) - 1] = 0;
		for i in m..(FIELD_BITS - 1) {
			let s = 1 << (i + 1);

			let mut j = (1 << m) - 1;
			while j < s {
				SKEW_FACTOR[j + s] = SKEW_FACTOR[j] ^ base[i];
				j += step;
			}
		}

		let idx = mul_table(base[m], LOG_TABLE[(base[m] ^ 1_u16) as usize]);
		base[m] = MODULO - LOG_TABLE[idx as usize];

		for i in (m + 1)..(FIELD_BITS - 1) {
			let b = LOG_TABLE[(base[i] as u16 ^ 1_u16) as usize] as u32 + base[m] as u32;
			let b = b % MODULO as u32;
			base[i] = mul_table(base[i], b as u16);
		}
	}
	for i in 0..(MODULO as usize) {
		SKEW_FACTOR[i] = LOG_TABLE[SKEW_FACTOR[i] as usize];
	}

	base[0] = MODULO - base[0];
	for i in 1..(FIELD_BITS - 1) {
		base[i] = ((MODULO as u32 - base[i] as u32 + base[i - 1] as u32) % MODULO as u32) as GFSymbol;
	}

	B[0] = 0;
	for i in 0..(FIELD_BITS - 1) {
		let depart = 1 << i;
		for j in 0..depart {
			B[j + depart] = ((B[j] as u32 + base[i] as u32) % MODULO as u32) as GFSymbol;
		}
	}

	mem_cpy(&mut LOG_WALSH[..], &LOG_TABLE[..]);
	LOG_WALSH[0] = 0;
	walsh(&mut LOG_WALSH[..], FIELD_SIZE);
}

// Encoding alg for k/n < 0.5: message is a power of two
fn encode_low(data: &[GFSymbol], k: usize, codeword: &mut [GFSymbol], n: usize) {
	assert!(k + k <= n);
	assert_eq!(codeword.len(), n);
	assert_eq!(data.len(), n);

	assert!(is_power_of_2(n));
	assert!(is_power_of_2(k));

	// k | n is guaranteed
	assert_eq!((n / k) * k, n);

	// move the data to the codeword
	mem_cpy(&mut codeword[0..], &data[0..]);

	// split after the first k
	let (codeword_first_k, codeword_skip_first_k) = codeword.split_at_mut(k);

	inverse_fft_in_novel_poly_basis(codeword_first_k, k, 0);

	// the first codeword is now the basis for the remaining transforms
	// denoted `M_topdash`

	for shift in (k..n).into_iter().step_by(k) {
		let codeword_at_shift = &mut codeword_skip_first_k[(shift - k)..shift];
		// copy `M_topdash` to the position we are currently at, the n transform
		mem_cpy(codeword_at_shift, codeword_first_k);
		fft_in_novel_poly_basis(codeword_at_shift, k, shift);
	}

	// restore `M` from the derived ones
	mem_cpy(&mut codeword[0..k], &data[0..k]);
}

fn mem_zero(zerome: &mut [GFSymbol]) {
	for i in 0..zerome.len() {
		zerome[i] = 0_u16;
	}
}

fn mem_cpy(dest: &mut [GFSymbol], src: &[GFSymbol]) {
	let sl = src.len();
	debug_assert_eq!(dest.len(), sl);
	for i in 0..sl {
		dest[i] = src[i];
	}
}

//data: message array. parity: parity array. mem: buffer(size>= n-k)
//Encoding alg for k/n>0.5: parity is a power of two.
fn encode_high(data: &[GFSymbol], k: usize, parity: &mut [GFSymbol], mem: &mut [GFSymbol], n: usize) {
	let t: usize = n - k;

	mem_zero(&mut parity[0..t]);

	let mut i = t;
	while i < n {
		mem_cpy(&mut mem[..t], &data[(i - t)..t]);

		inverse_fft_in_novel_poly_basis(mem, t, i);
		for j in 0..t {
			parity[j] ^= mem[j];
		}
		i += t;
	}
	fft_in_novel_poly_basis(parity, t, 0);
}

// Compute the evaluations of the error locator polynomial
// `fn decode_init`
// since this has only to be called once per reconstruction
fn eval_error_polynomial(erasure: &[bool], log_walsh2: &mut [GFSymbol], n: usize) {
	let z = std::cmp::min(n,erasure.len());
	for i in 0..z {
		log_walsh2[i] = erasure[i] as GFSymbol;
	}
	for i in z..N {
		log_walsh2[i] = 0 as GFSymbol;
	}
	walsh(log_walsh2, FIELD_SIZE);
	for i in 0..n {
		let tmp = log_walsh2[i] as u32 * unsafe { LOG_WALSH[i] } as u32;
		log_walsh2[i] = (tmp % MODULO as u32) as GFSymbol;
	}
	walsh(log_walsh2, FIELD_SIZE);
	for i in 0..z {
		if erasure[i] {
			log_walsh2[i] = MODULO - log_walsh2[i];
		}
	}
}

fn decode_main(codeword: &mut [GFSymbol], k: usize, erasure: &[bool], log_walsh2: &[GFSymbol], n: usize) {
	assert!(codeword.len() >= k);
	assert_eq!(codeword.len(), n);
	assert!(erasure.len() >= k);
	assert_eq!(erasure.len(), n);

	// technically we only need to recover
	// the first `k` instead of all `n` which
	// would include parity chunks.
	let recover_up_to = n;

	for i in 0..n {
		codeword[i] = if erasure[i] { 0_u16 } else { mul_table(codeword[i], log_walsh2[i]) };
	}
	inverse_fft_in_novel_poly_basis(codeword, n, 0);

	//formal derivative
	for i in (0..n).into_iter().step_by(2) {
		let b = MODULO - unsafe { B[i >> 1] };
		codeword[i] = mul_table(codeword[i], b);
		codeword[i + 1] = mul_table(codeword[i + 1], b);
	}

	formal_derivative(codeword, n);

	for i in (0..n).into_iter().step_by(2) {
		let b = unsafe { B[i >> 1] };
		codeword[i] = mul_table(codeword[i], b);
		codeword[i + 1] = mul_table(codeword[i + 1], b);
	}

	fft_in_novel_poly_basis(codeword, n, 0);

	for i in 0..recover_up_to {
		codeword[i] = if erasure[i] { mul_table(codeword[i], log_walsh2[i]) } else { 0_u16 };
	}
}

const N: usize = 32;
const K: usize = 4;

use itertools::Itertools;

pub fn encode(data: &[u8]) -> Vec<WrappedShard> {
	unsafe { init() };

	// must be power of 2
	let l = log2(data.len());
	let l = 1 << l;
	let l = if l >= data.len() { l } else { l << 1 };
	assert!(l >= data.len());
	assert!(is_power_of_2(l));
	assert!(is_power_of_2(N), "Algorithm only works for 2^m sizes for N");
	assert!(is_power_of_2(K), "Algorithm only works for 2^m sizes for K");

	// pad the incoming data with trailing 0s
	let zero_bytes_to_add = dbg!(l) - dbg!(data.len());
	let data: Vec<GFSymbol> = data
		.into_iter()
		.copied()
		.chain(std::iter::repeat(0u8).take(zero_bytes_to_add))
		.tuple_windows()
		.step_by(2)
		.map(|(a, b)| (b as u16) << 8 | a as u16)
		.collect::<Vec<GFSymbol>>();

	// assert_eq!(K, data.len());
	assert_eq!(data.len() * 2, l + zero_bytes_to_add);

	// two bytes make one `l / 2`
	let l = l / 2;
	assert_eq!(l, N, "For now we only want to test of variants that don't have to be 0 padded");
	let mut codeword = data.clone();
	assert_eq!(codeword.len(), N);

	assert!(K <= N / 2);
	// if K + K > N {
	// 	let (data_till_t, data_skip_t) = data.split_at_mut(N - K);
	// 	encode_high(data_skip_t, K, data_till_t, &mut codeword[..], N);
	// } else {
	encode_low(&data[..], K, &mut codeword[..], N);
	// }

	println!("Codeword:");
	for i in 0..N {
		print!("{:04x} ", codeword[i]);
	}
	println!("");

	// XXX currently this is only done for one codeword!

	let shards = (0..N)
		.into_iter()
		.map(|i| {
			WrappedShard::new({
				let arr = codeword[i].to_le_bytes();
				arr.to_vec()
			})
		})
		.collect::<Vec<WrappedShard>>();

	shards
}

pub fn reconstruct(received_shards: Vec<Option<WrappedShard>>) -> Option<Vec<u8>> {
	unsafe { init_dec() };

	// collect all `None` values
	let mut erased_count = 0;
	let erasures = received_shards
		.iter()
		.map(|x| x.is_none())
		.inspect(|v| {
			if *v {
				erased_count += 1;
			}
		})
		.collect::<Vec<bool>>();

	// The recovered _data_ chunks AND parity chunks
	let mut recovered: Vec<GFSymbol> = std::iter::repeat(0u16).take(N).collect();

	// get rid of all `None`s
	let mut codeword = received_shards
		.into_iter()
		.enumerate()
		.map(|(idx, wrapped)| {
			// fill the gaps with `0_u16` codewords
			if let Some(wrapped) = wrapped {
				let v: &[[u8; 2]] = wrapped.as_ref();
				(idx, u16::from_le_bytes(v[0]))
			} else {
				(idx, 0_u16)
			}
		})
		.map(|(idx, codeword)| {
			// copy the good messages (here it's just one codeword/u16 right now)
			if idx < N {
				recovered[idx] = codeword;
			}
			codeword
		})
		.collect::<Vec<u16>>();

	// filled up the remaining spots with 0s
	assert_eq!(codeword.len(), N);

	let recover_up_to = N; // the first k would suffice for the original k message codewords

	//---------Erasure decoding----------------
	let mut log_walsh2: [GFSymbol; FIELD_SIZE] = [0_u16; FIELD_SIZE];

	// Evaluate error locator polynomial
	eval_error_polynomial(&erasures[..], &mut log_walsh2[..], FIELD_SIZE);

	//---------main processing----------
	decode_main(&mut codeword[..], recover_up_to, &erasures[..], &log_walsh2[..], N);

	println!("Decoded result:");
	for idx in 0..N {
		if erasures[idx] {
			print!("{:04x} ", codeword[idx]);
			recovered[idx] = codeword[idx];
		} else {
			print!("XXXX ");
		};
	}

	let recovered = unsafe {
		// TODO assure this does not leak memory
		let x = from_raw_parts(recovered.as_ptr() as *const u8, recovered.len() * 2);
		std::mem::forget(recovered);
		x
	};
	Some(recovered.to_vec())
}

#[cfg(test)]
mod test {
	use rand::seq::index::IndexVec;

	use super::*;

	fn print_sha256(txt: &'static str, data: &[GFSymbol]) {
		use sha2::Digest;
		let data = unsafe { ::std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * 2) };

		let mut digest = sha2::Sha256::new();
		digest.update(data);
		println!("sha256(rs|{}):", txt);
		for byte in digest.finalize().into_iter() {
			print!("{:02x}", byte);
		}
		println!("")
	}

	/// Generate a random index
	fn rand_gf_element() -> GFSymbol {
		use rand::distributions::{Distribution, Uniform};
		use rand::thread_rng;

		let mut rng = thread_rng();
		let uni = Uniform::<GFSymbol>::new_inclusive(0, MODULO);
		uni.sample(&mut rng)
	}

	#[test]
	fn flt_back_and_forth() {
		const N: usize = 128;
		const K: usize = 32;
		let mut data = (0..N).into_iter().map(|_x| rand_gf_element()).collect::<Vec<GFSymbol>>();
		let expected = data.clone();

		fft_in_novel_poly_basis(&mut data, N, K);

		// make sure something is done
		assert!(data.iter().zip(expected.iter()).filter(|(a, b)| { a != b }).count() > 0);

		inverse_fft_in_novel_poly_basis(&mut data, N, K);

		itertools::assert_equal(data, expected);
	}

	#[test]
	fn flt_rountrip_small() {
		const N: usize = 16;
		const EXPECTED: [GFSymbol; N] = [1, 2, 3, 5, 8, 13, 21, 44, 65, 0, 0xFFFF, 2, 3, 5, 7, 11];

		let mut data = EXPECTED.clone();

		fft_in_novel_poly_basis(&mut data, N, N / 4);

		println!("novel basis(rust):");
		data.iter().for_each(|sym| {
			print!(" {:04X}", sym);
		});
		println!("");

		inverse_fft_in_novel_poly_basis(&mut data, N, N / 4);
		itertools::assert_equal(data.iter(), EXPECTED.iter());
	}

	#[test]
	fn ported_c_test() {
		unsafe {
			init(); //fill log table and exp table
			init_dec(); //compute factors used in erasure decoder
		}

		//-----------Generating message----------
		//message array
		let mut data: [GFSymbol; N] = [0; N];

		for i in 0..K {
			//filled with random numbers
			data[i] = (i * i % MODULO as usize) as u16;
			// data[i] = rand_gf_element();
		}

		assert_eq!(data.len(), N);

		println!("Message(Last n-k are zeros): ");
		for i in 0..K {
			print!("{:04x} ", data[i]);
		}
		println!("");
		print_sha256("data", &data[..]);

		//---------encoding----------
		let mut codeword = [0_u16; N];

		if K + K > N && false {
			let (data_till_t, data_skip_t) = data.split_at_mut(N - K);
			encode_high(data_skip_t, K, data_till_t, &mut codeword[..], N);
		} else {
			encode_low(&data[..], K, &mut codeword[..], N);
		}

		// println!("Codeword:");
		// for i in K..(K+100) {
		// print!("{:04x} ", codeword[i]);
		// }
		// println!("");

		print_sha256("encoded", &codeword);

		//--------erasure simulation---------

		// Array indicating erasures
		let mut erasure = [false; N];

		let erasures_iv = if false {
			// erase random `(N-K)` codewords
			let mut rng = rand::thread_rng();
			let erasures_iv: IndexVec = rand::seq::index::sample(&mut rng, N, N - K);

			erasures_iv
		} else {
			IndexVec::from((0..(N - K)).into_iter().collect::<Vec<usize>>())
		};
		assert_eq!(erasures_iv.len(), N - K);

		for i in erasures_iv {
			//erasure codeword symbols
			erasure[i] = true;
			codeword[i] = 0 as GFSymbol;
		}

		print_sha256("erased", &codeword);

		//---------Erasure decoding----------------
		let mut log_walsh2: [GFSymbol; FIELD_SIZE] = [0_u16; FIELD_SIZE];

		eval_error_polynomial(&erasure[..], &mut log_walsh2[..], FIELD_SIZE);

		print_sha256("log_walsh2", &log_walsh2);

		decode_main(&mut codeword[..], K, &erasure[..], &log_walsh2[..], N);

		print_sha256("decoded", &codeword[0..K]);

		println!("Decoded result:");
		for i in 0..N {
			// the data word plus a few more
			print!("{:04x} ", codeword[i]);
		}
		println!("");

		for i in 0..K {
			//Check the correctness of the result
			if data[i] != codeword[i] {
				println!("🐍🐍🐍🐍🐍🐍🐍🐍🐍🐍🐍🐍🐍🐍🐍🐍🐍");
				panic!("Decoding ERROR! value at [{}] should={:04x} vs is={:04x}", i, data[i], codeword[i]);
			}
		}
		println!(r#">>>>>>>>> 🎉🎉🎉🎉
>>>>>>>>> > Decoding is **SUCCESS** ful! 🎈
>>>>>>>>>"#);

	}
}
