pub enum Cell<'a> {
    Atom(&'a [u8]),
    Pair(Node<'a>, Node<'a>),
}

pub trait NodeAllocator<'a> {
    fn new_atom(&mut self, blob: &[u8]) -> Node<'a>;
    fn new_pair(&mut self, left: Node<'a>, right: Node<'a>) -> Node<'a>;
}

type Node<'a> = &'a dyn Fn() -> &'a Cell<'a>;

struct SimpleNodeAllocator<'a> {
    atoms: Vec<Vec<u8>>,
    nodes: Vec<Box<Node<'a>>>,
}

impl<'a> SimpleNodeAllocator<'a> {
    fn new_node(&mut self, cell: Cell) -> Node<'a> {
        self.nodes.push(Box::new(&|| &cell));
        let n: Node<'a> = self.nodes.last().unwrap().into();
        &n
    }
}

impl<'a> NodeAllocator<'a> for SimpleNodeAllocator<'a> {
    fn new_atom(&mut self, blob: &[u8]) -> Node<'a> {
        let v: Vec<u8> = blob.into();
        self.atoms.push(v);
        self.new_node(Cell::Atom(&v))
    }
    fn new_pair(&mut self, left: Node<'a>, right: Node<'a>) -> Node<'a> {
        let cell = Cell::Pair(left, right);
        self.new_node(cell)
    }
}

/////////////////////////////////////////

const MAX_SINGLE_BYTE: u8 = 0x7f;
const CONS_BOX_MARKER: u8 = 0xff;

pub fn node_from_bytes<'a>(
    mut buffer: &'a [u8],
    allocator: &mut dyn NodeAllocator<'a>,
) -> std::io::Result<(Node<'a>, &'a [u8])> {
    let b: u8 = buffer[0];

    if b <= MAX_SINGLE_BYTE {
        return Ok((allocator.new_atom(&buffer[0..1]), &buffer[1..]));
    }

    buffer = &buffer[1..];

    if b == CONS_BOX_MARKER {
        let (v1, buffer) = node_from_bytes(buffer, allocator)?;
        let (v2, buffer) = node_from_bytes(buffer, allocator)?;
        return Ok((allocator.new_pair(v1, v2), buffer));
    }
    let (blob_size, buffer) = decode_size(buffer)?;
    let blob: &[u8] = &buffer[0..blob_size];
    Ok((allocator.new_atom(blob), &buffer[blob_size..]))
}

fn decode_size(buffer: &[u8]) -> std::io::Result<(usize, &[u8])> {
    let mut bit_count = 0;
    let mut bit_mask: u8 = 0x80;
    let mut b = buffer[0];
    while b & bit_mask != 0 {
        bit_count += 1;
        b &= 0xff ^ bit_mask;
        bit_mask >>= 1;
    }
    // need to convert size_blob to an int
    let mut v: usize = b as usize;
    for b in buffer[1..bit_count].iter() {
        v <<= 8;
        v += *b as usize;
    }
    let bytes_to_read = v;
    Ok((bytes_to_read, &buffer[bit_count..]))
}
