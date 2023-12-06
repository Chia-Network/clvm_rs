use crate::{allocator::NodePtr, Allocator};

use clvm_traits::{ClvmDecoder, FromClvm, FromClvmError};

pub trait FromNodePtr {
    fn from_node_ptr(a: &Allocator, node: NodePtr) -> Result<Self, FromClvmError>
    where
        Self: Sized;
}

impl<T> FromNodePtr for T
where
    T: FromClvm<NodePtr>,
{
    fn from_node_ptr(a: &Allocator, node: NodePtr) -> Result<Self, FromClvmError>
    where
        Self: Sized,
    {
        T::from_clvm(a, node)
    }
}

impl FromClvm<NodePtr> for NodePtr {
    fn from_clvm(
        _decoder: &impl ClvmDecoder<Node = NodePtr>,
        node: NodePtr,
    ) -> Result<Self, FromClvmError> {
        Ok(node)
    }
}

#[cfg(test)]
mod tests {
    use crate::{allocator::NodePtr, serde::node_from_bytes, Allocator};

    use super::*;

    fn decode<T>(a: &mut Allocator, hex: &str) -> Result<T, FromClvmError>
    where
        T: FromClvm<NodePtr>,
    {
        let bytes = hex::decode(hex).unwrap();
        let actual = node_from_bytes(a, &bytes).unwrap();
        T::from_clvm(a, actual)
    }

    #[test]
    fn test_nodeptr() {
        let a = &mut Allocator::new();
        let ptr = a.one();
        assert_eq!(NodePtr::from_clvm(a, ptr).unwrap(), ptr);
    }

    #[test]
    fn test_primitives() {
        let a = &mut Allocator::new();
        assert_eq!(decode(a, "80"), Ok(0u8));
        assert_eq!(decode(a, "80"), Ok(0i8));
        assert_eq!(decode(a, "05"), Ok(5u8));
        assert_eq!(decode(a, "05"), Ok(5u32));
        assert_eq!(decode(a, "05"), Ok(5i32));
        assert_eq!(decode(a, "81e5"), Ok(-27i32));
        assert_eq!(decode(a, "80"), Ok(-0));
        assert_eq!(decode(a, "8180"), Ok(-128i8));
    }

    #[test]
    fn test_pair() {
        let a = &mut Allocator::new();
        assert_eq!(decode(a, "ff0502"), Ok((5, 2)));
        assert_eq!(decode(a, "ff81b8ff8301600980"), Ok((-72, (90121, ()))));
        assert_eq!(
            decode(a, "ffff80ff80ff80ffff80ff80ff80808080"),
            Ok((((), ((), ((), (((), ((), ((), ()))), ())))), ()))
        );
    }

    #[test]
    fn test_nil() {
        let a = &mut Allocator::new();
        assert_eq!(decode(a, "80"), Ok(()));
    }

    #[test]
    fn test_array() {
        let a = &mut Allocator::new();
        assert_eq!(decode(a, "ff01ff02ff03ff0480"), Ok([1, 2, 3, 4]));
        assert_eq!(decode(a, "80"), Ok([] as [i32; 0]));
    }

    #[test]
    fn test_vec() {
        let a = &mut Allocator::new();
        assert_eq!(decode(a, "ff01ff02ff03ff0480"), Ok(vec![1, 2, 3, 4]));
        assert_eq!(decode(a, "80"), Ok(Vec::<i32>::new()));
    }

    #[test]
    fn test_option() {
        let a = &mut Allocator::new();
        assert_eq!(decode(a, "8568656c6c6f"), Ok(Some("hello".to_string())));
        assert_eq!(decode(a, "80"), Ok(None::<String>));

        // Empty strings get decoded as None instead, since both values are represented by nil bytes.
        // This could be considered either intended behavior or not, depending on the way it's used.
        assert_ne!(decode(a, "80"), Ok(Some("".to_string())));
    }

    #[test]
    fn test_string() {
        let a = &mut Allocator::new();
        assert_eq!(decode(a, "8568656c6c6f"), Ok("hello".to_string()));
        assert_eq!(decode(a, "80"), Ok("".to_string()));
    }
}
