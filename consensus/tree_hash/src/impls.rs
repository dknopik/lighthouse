use super::*;
use ethereum_types::{H160, H256, U128, U256};

fn int_to_hash256(int: u64) -> Hash256 {
    let mut bytes = [0; HASHSIZE];
    bytes[0..8].copy_from_slice(&int.to_le_bytes());
    Hash256::from_slice(&bytes)
}

macro_rules! impl_for_bitsize {
    ($type: ident, $bit_size: expr) => {
        impl TreeHash for $type {
            fn tree_hash_type() -> TreeHashType {
                TreeHashType::Basic
            }

            fn tree_hash_packed_encoding(&self) -> Vec<u8> {
                self.to_le_bytes().to_vec()
            }

            fn tree_hash_packing_factor() -> usize {
                HASHSIZE / ($bit_size / 8)
            }

            #[allow(clippy::cast_lossless)] // Lint does not apply to all uses of this macro.
            fn tree_hash_root(&self) -> Hash256 {
                int_to_hash256(*self as u64)
            }
        }
    };
}

impl_for_bitsize!(u8, 8);
impl_for_bitsize!(u16, 16);
impl_for_bitsize!(u32, 32);
impl_for_bitsize!(u64, 64);
impl_for_bitsize!(usize, 64);

impl TreeHash for bool {
    fn tree_hash_type() -> TreeHashType {
        TreeHashType::Basic
    }

    fn tree_hash_packed_encoding(&self) -> Vec<u8> {
        (*self as u8).tree_hash_packed_encoding()
    }

    fn tree_hash_packing_factor() -> usize {
        u8::tree_hash_packing_factor()
    }

    fn tree_hash_root(&self) -> Hash256 {
        int_to_hash256(*self as u64)
    }
}

impl<const LEN: usize> TreeHash for [u8; LEN] {
    fn tree_hash_type() -> TreeHashType {
        TreeHashType::Vector
    }

    fn tree_hash_packed_encoding(&self) -> Vec<u8> {
        unreachable!("Vector should never be packed.")
    }

    fn tree_hash_packing_factor() -> usize {
        unreachable!("Vector should never be packed.")
    }

    fn tree_hash_root(&self) -> Hash256 {
        if LEN < 32 {
            let mut result = [0; 32];
            result[0..LEN].copy_from_slice(&self[..]);
            Hash256::from_slice(&result)
        } else if LEN == 32 {
            Hash256::from_slice(self)
        } else {
            let mut hasher = MerkleHasher::with_leaves((LEN + 31) / 32);

            hasher
                .write(self)
                .expect("exact length array should not have too many leaves");

            hasher
                .finish()
                .expect("exact length array should not have too many leaves")
        }
    }
}

impl TreeHash for U128 {
    fn tree_hash_type() -> TreeHashType {
        TreeHashType::Basic
    }

    fn tree_hash_packed_encoding(&self) -> Vec<u8> {
        let mut result = vec![0; 16];
        self.to_little_endian(&mut result);
        result
    }

    fn tree_hash_packing_factor() -> usize {
        2
    }

    fn tree_hash_root(&self) -> Hash256 {
        let mut result = [0; HASHSIZE];
        self.to_little_endian(&mut result[0..16]);
        Hash256::from_slice(&result)
    }
}

impl TreeHash for U256 {
    fn tree_hash_type() -> TreeHashType {
        TreeHashType::Basic
    }

    fn tree_hash_packed_encoding(&self) -> Vec<u8> {
        let mut result = vec![0; 32];
        self.to_little_endian(&mut result);
        result
    }

    fn tree_hash_packing_factor() -> usize {
        1
    }

    fn tree_hash_root(&self) -> Hash256 {
        let mut result = [0; 32];
        self.to_little_endian(&mut result[..]);
        Hash256::from_slice(&result)
    }
}

impl TreeHash for H160 {
    fn tree_hash_type() -> TreeHashType {
        TreeHashType::Vector
    }

    fn tree_hash_packed_encoding(&self) -> Vec<u8> {
        let mut result = vec![0; 32];
        result[0..20].copy_from_slice(self.as_bytes());
        result
    }

    fn tree_hash_packing_factor() -> usize {
        1
    }

    fn tree_hash_root(&self) -> Hash256 {
        let mut result = [0; 32];
        result[0..20].copy_from_slice(self.as_bytes());
        Hash256::from_slice(&result)
    }
}

impl TreeHash for H256 {
    fn tree_hash_type() -> TreeHashType {
        TreeHashType::Vector
    }

    fn tree_hash_packed_encoding(&self) -> Vec<u8> {
        self.as_bytes().to_vec()
    }

    fn tree_hash_packing_factor() -> usize {
        1
    }

    fn tree_hash_root(&self) -> Hash256 {
        *self
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn bool() {
        let mut true_bytes: Vec<u8> = vec![1];
        true_bytes.append(&mut vec![0; 31]);

        let false_bytes: Vec<u8> = vec![0; 32];

        assert_eq!(true.tree_hash_root().as_bytes(), true_bytes.as_slice());
        assert_eq!(false.tree_hash_root().as_bytes(), false_bytes.as_slice());
    }

    #[test]
    fn int_to_bytes() {
        assert_eq!(int_to_hash256(0).as_bytes(), &[0; 32]);
        assert_eq!(
            int_to_hash256(1).as_bytes(),
            &[
                1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0
            ]
        );
        assert_eq!(
            int_to_hash256(u64::max_value()).as_bytes(),
            &[
                255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ]
        );
    }

    #[test]
    fn aaaaah() {
        assert_eq!([1, 2, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0].tree_hash_root(),
                   [1, 2, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0].tree_hash_root())
    }
}
