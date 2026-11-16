//! Reader for an installer receipt's bill of materials (`/var/db/receipts/*.bom`),
//! the file list `pkgutil --files` prints (ADR-0064). Read only, and only the
//! names, kinds and parents of the recorded paths; nothing else in the file is
//! interpreted.
//!
//! Apple does not document the format. The layout below is the long-standing
//! `BOMStore` layout, checked on macOS 26 against `pkgutil --files` for every
//! third-party receipt on a test Mac (identical lists, apart from Unicode
//! normalisation, which this reader leaves byte-exact):
//!
//! - All integers are big-endian. The header holds the magic `BOMStore`, a
//!   version, a block count, and the offsets and lengths of the block table
//!   and of the named variables.
//! - The block table is a count and then `(offset, length)` pairs; block `i`
//!   is entry `i`, and block 0 is null.
//! - The variable `Paths` names a block holding a `tree` header whose child is
//!   the root node of a B-tree of paths. A node is `is_leaf: u16`,
//!   `count: u16`, `forward: u32`, `backward: u32`, then `count` pairs of
//!   block indices. An inner node's first pair leads down; leaves are chained
//!   by `forward`.
//! - In a leaf pair, the first block holds the path's ID and a further block
//!   whose first byte is its kind (1 file, 2 directory, 3 symbolic link, 4
//!   device); the second holds its parent's ID and its NUL-terminated name.
//!   The top entry is `.` with parent 0.
//!
//! The file is untrusted input: every offset and length is checked, every walk
//! is bounded, and anything inconsistent fails the whole file
//! ([`BomError`]) rather than yielding a partial or guessed list.

use std::collections::{HashMap, HashSet};

/// Why a bill of materials could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("not a readable bill of materials: {0}")]
pub struct BomError(&'static str);

/// The deepest path or tree accepted.
const MAX_DEPTH: usize = 4096;

/// A path's kind, as recorded.
const KIND_FILE: u8 = 1;
const KIND_DIRECTORY: u8 = 2;

/// One recorded path: its parent's ID, its name and its kind.
#[derive(Debug, Clone, Copy)]
struct Entry<'a> {
    parent: u32,
    name: &'a [u8],
    kind: u8,
}

struct Store<'a> {
    data: &'a [u8],
    table: &'a [u8],
}

fn u16_at(bytes: &[u8], at: usize) -> Result<u16, BomError> {
    bytes
        .get(at..at + 2)
        .and_then(|b| b.try_into().ok())
        .map(u16::from_be_bytes)
        .ok_or(BomError("a field runs past its block"))
}

fn u32_at(bytes: &[u8], at: usize) -> Result<u32, BomError> {
    bytes
        .get(at..at + 4)
        .and_then(|b| b.try_into().ok())
        .map(u32::from_be_bytes)
        .ok_or(BomError("a field runs past its block"))
}

fn usize_at(bytes: &[u8], at: usize) -> Result<usize, BomError> {
    usize::try_from(u32_at(bytes, at)?).map_err(|_| BomError("an offset does not fit"))
}

impl<'a> Store<'a> {
    fn open(data: &'a [u8]) -> Result<Self, BomError> {
        if data.get(..8) != Some(b"BOMStore".as_slice()) {
            return Err(BomError("no BOMStore magic"));
        }
        if u32_at(data, 8)? != 1 {
            return Err(BomError("an unknown version"));
        }
        let index = usize_at(data, 16)?;
        let index_length = usize_at(data, 20)?;
        let table = data
            .get(index..index.saturating_add(index_length))
            .ok_or(BomError("the block table lies outside the file"))?;
        let count = usize_at(table, 0)?;
        let table = table
            .get(4..count.saturating_mul(8).saturating_add(4))
            .ok_or(BomError("the block table is shorter than its count"))?;
        Ok(Self { data, table })
    }

    fn blocks(&self) -> usize {
        self.table.len() / 8
    }

    fn block(&self, index: u32) -> Result<&'a [u8], BomError> {
        let at = usize::try_from(index).map_err(|_| BomError("a block index does not fit"))?;
        if at == 0 || at >= self.blocks() {
            return Err(BomError("a block index is out of range"));
        }
        let offset = usize_at(self.table, at * 8)?;
        let length = usize_at(self.table, at * 8 + 4)?;
        self.data
            .get(offset..offset.saturating_add(length))
            .ok_or(BomError("a block lies outside the file"))
    }

    /// The block a named variable points at.
    fn variable(&self, name: &[u8]) -> Result<&'a [u8], BomError> {
        let vars = usize_at(self.data, 24)?;
        let count = usize_at(self.data, vars)?;
        let mut at = vars + 4;
        for _ in 0..count {
            let index = u32_at(self.data, at)?;
            let length = usize::from(
                *self
                    .data
                    .get(at + 4)
                    .ok_or(BomError("a variable runs past the file"))?,
            );
            let found = self
                .data
                .get(at + 5..at + 5 + length)
                .ok_or(BomError("a variable runs past the file"))?;
            if found == name {
                return self.block(index);
            }
            at += 5 + length;
        }
        Err(BomError("no Paths variable"))
    }

    /// Every recorded path, by ID.
    fn entries(&self) -> Result<HashMap<u32, Entry<'a>>, BomError> {
        let tree = self.variable(b"Paths")?;
        if tree.get(..4) != Some(b"tree".as_slice()) {
            return Err(BomError("Paths is not a tree"));
        }
        let mut node = self.block(u32_at(tree, 8)?)?;
        let mut depth = 0;
        while u16_at(node, 0)? == 0 {
            depth += 1;
            if depth > MAX_DEPTH {
                return Err(BomError("the tree is too deep"));
            }
            node = self.block(u32_at(node, 12)?)?;
        }
        let mut entries = HashMap::new();
        // Each leaf is visited at most once, so a cycle cannot loop forever.
        for _ in 0..self.blocks() {
            if u16_at(node, 0)? == 0 {
                return Err(BomError("an inner node among the leaves"));
            }
            for pair in 0..usize::from(u16_at(node, 2)?) {
                let info = self.block(u32_at(node, 12 + pair * 8)?)?;
                let file = self.block(u32_at(node, 16 + pair * 8)?)?;
                let id = u32_at(info, 0)?;
                let kind = *self
                    .block(u32_at(info, 4)?)?
                    .first()
                    .ok_or(BomError("an empty path record"))?;
                let parent = u32_at(file, 0)?;
                let name = file.get(4..).unwrap_or_default();
                let name = name.split(|b| *b == 0).next().unwrap_or_default();
                if entries.insert(id, Entry { parent, name, kind }).is_some() {
                    return Err(BomError("two paths share an ID"));
                }
            }
            match u32_at(node, 4)? {
                0 => return Ok(entries),
                next => node = self.block(next)?,
            }
        }
        Err(BomError("the leaves form a cycle"))
    }
}

/// The application bundles a bill of materials records: each outermost
/// directory named `*.app` whose `Contents/Info.plist` it records as a file.
/// Paths are relative to the package's install prefix, without `./`, sorted.
/// An empty path is the prefix itself, for a package whose prefix is its
/// bundle (`Applications/Tailscale.app`) and which records `./Contents/Info.plist`;
/// the caller checks that the prefix is a bundle. A directory alone is never
/// enough: packages share directories.
///
/// # Errors
///
/// [`BomError`] if the file is not a bill of materials this reader
/// understands.
pub fn bundles(bom: &[u8]) -> Result<Vec<Vec<u8>>, BomError> {
    let entries = Store::open(bom)?.entries()?;
    let is_dir = |id: u32, name: &dyn Fn(&[u8]) -> bool| {
        entries
            .get(&id)
            .is_some_and(|e| e.kind == KIND_DIRECTORY && name(e.name))
    };
    let found: HashSet<u32> = entries
        .values()
        .filter(|e| e.kind == KIND_FILE && e.name == b"Info.plist")
        .filter(|e| is_dir(e.parent, &|name| name == b"Contents"))
        .filter_map(|e| entries.get(&e.parent).map(|contents| contents.parent))
        .filter(|&bundle| {
            let top = entries.get(&bundle).is_some_and(|e| e.parent == 0);
            is_dir(bundle, &|name| {
                (top && name == b".")
                    || (name.len() > 4 && name[name.len() - 4..].eq_ignore_ascii_case(b".app"))
            })
        })
        .collect();
    let mut paths: Vec<Vec<u8>> = found
        .iter()
        .filter_map(|&bundle| {
            // Names from the bundle up to the top, or `None` for a bundle
            // inside another or a chain that does not reach the top.
            let mut names = Vec::new();
            let mut at = bundle;
            for _ in 0..MAX_DEPTH {
                if at == 0 {
                    break;
                }
                if at != bundle && found.contains(&at) {
                    return None;
                }
                let entry = entries.get(&at)?;
                names.push(entry.name);
                at = entry.parent;
            }
            if at != 0 {
                return None;
            }
            names.reverse();
            if names.first() == Some(&b".".as_slice()) {
                names.remove(0);
            }
            let unsafe_name =
                |n: &&[u8]| n.is_empty() || *n == b"." || *n == b".." || n.contains(&b'/');
            if names.iter().any(unsafe_name) {
                return None;
            }
            Some(names.join(&b'/'))
        })
        .collect();
    paths.sort();
    Ok(paths)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Builds a bill of materials holding `paths` (`(id, parent, name, kind)`),
    /// with the leaves split in two to exercise the forward chain.
    pub(crate) fn build(paths: &[(u32, u32, &str, u8)]) -> Vec<u8> {
        let mut data = vec![0u8; 512];
        let mut blocks: Vec<(u32, u32)> = vec![(0, 0)];
        let mut block = |data: &mut Vec<u8>, bytes: &[u8]| -> u32 {
            let offset = u32::try_from(data.len()).unwrap_or_else(|_| unreachable!());
            data.extend_from_slice(bytes);
            let length = u32::try_from(bytes.len()).unwrap_or_else(|_| unreachable!());
            blocks.push((offset, length));
            u32::try_from(blocks.len() - 1).unwrap_or_else(|_| unreachable!())
        };
        let mut pairs = Vec::new();
        for &(id, parent, name, kind) in paths {
            let kind_block = block(&mut data, &[kind, 0, 0, 0]);
            let mut info = id.to_be_bytes().to_vec();
            info.extend_from_slice(&kind_block.to_be_bytes());
            let info = block(&mut data, &info);
            let mut file = parent.to_be_bytes().to_vec();
            file.extend_from_slice(name.as_bytes());
            file.push(0);
            let file = block(&mut data, &file);
            pairs.push((info, file));
        }
        let leaf = |pairs: &[(u32, u32)], forward: u32| {
            let mut node = 1u16.to_be_bytes().to_vec();
            let count = u16::try_from(pairs.len()).unwrap_or_else(|_| unreachable!());
            node.extend_from_slice(&count.to_be_bytes());
            node.extend_from_slice(&forward.to_be_bytes());
            node.extend_from_slice(&0u32.to_be_bytes());
            for (a, b) in pairs {
                node.extend_from_slice(&a.to_be_bytes());
                node.extend_from_slice(&b.to_be_bytes());
            }
            node
        };
        let (first, second) = pairs.split_at(pairs.len() / 2);
        let second = block(&mut data, &leaf(second, 0));
        let first = block(&mut data, &leaf(first, second));
        let mut root = 0u16.to_be_bytes().to_vec();
        root.extend_from_slice(&1u16.to_be_bytes());
        root.extend_from_slice(&[0; 8]);
        root.extend_from_slice(&first.to_be_bytes());
        root.extend_from_slice(&0u32.to_be_bytes());
        let root = block(&mut data, &root);
        let mut tree = b"tree".to_vec();
        tree.extend_from_slice(&1u32.to_be_bytes());
        tree.extend_from_slice(&root.to_be_bytes());
        tree.extend_from_slice(&[0; 9]);
        let tree = block(&mut data, &tree);
        let vars = u32::try_from(data.len()).unwrap_or_else(|_| unreachable!());
        data.extend_from_slice(&1u32.to_be_bytes());
        data.extend_from_slice(&tree.to_be_bytes());
        data.push(5);
        data.extend_from_slice(b"Paths");
        let index = u32::try_from(data.len()).unwrap_or_else(|_| unreachable!());
        let count = u32::try_from(blocks.len()).unwrap_or_else(|_| unreachable!());
        data.extend_from_slice(&count.to_be_bytes());
        for (offset, length) in &blocks {
            data.extend_from_slice(&offset.to_be_bytes());
            data.extend_from_slice(&length.to_be_bytes());
        }
        let index_length = u32::try_from(data.len()).unwrap_or_else(|_| unreachable!()) - index;
        data[..8].copy_from_slice(b"BOMStore");
        for (at, value) in [
            (8, 1),
            (12, count),
            (16, index),
            (20, index_length),
            (24, vars),
        ] {
            data[at..at + 4].copy_from_slice(&u32::to_be_bytes(value));
        }
        data
    }

    const DIR: u8 = KIND_DIRECTORY;
    const FILE: u8 = KIND_FILE;

    #[test]
    fn outermost_bundles_with_a_recorded_info_plist_are_found() {
        let bom = build(&[
            (1, 0, ".", DIR),
            (2, 1, "Foo.app", DIR),
            (3, 2, "Contents", DIR),
            (4, 3, "Info.plist", FILE),
            (5, 3, "Helper.app", DIR),
            (6, 5, "Contents", DIR),
            (7, 6, "Info.plist", FILE),
            // A directory with no recorded Info.plist is not a bundle.
            (8, 1, "Shared.app", DIR),
            (9, 8, "Contents", DIR),
            // A look-alike that is a file, not a directory.
            (10, 1, "Tool", DIR),
            (11, 10, "Fake.app", FILE),
            (12, 1, "Sub", DIR),
            (13, 12, "Bar.APP", DIR),
            (14, 13, "Contents", DIR),
            (15, 14, "Info.plist", FILE),
        ]);
        assert_eq!(
            bundles(&bom),
            Ok(vec![b"Foo.app".to_vec(), b"Sub/Bar.APP".to_vec()])
        );
    }

    #[test]
    fn a_package_whose_prefix_is_its_bundle_records_the_top() {
        let bom = build(&[
            (1, 0, ".", DIR),
            (2, 1, "Contents", DIR),
            (3, 2, "Info.plist", FILE),
            (4, 2, "Helper.app", DIR),
            (5, 4, "Contents", DIR),
            (6, 5, "Info.plist", FILE),
        ]);
        assert_eq!(bundles(&bom), Ok(vec![Vec::new()]));
    }

    #[test]
    fn an_info_plist_that_is_a_directory_or_link_is_not_enough() {
        let bom = build(&[
            (1, 0, ".", DIR),
            (2, 1, "Foo.app", DIR),
            (3, 2, "Contents", DIR),
            (4, 3, "Info.plist", 3),
        ]);
        assert_eq!(bundles(&bom), Ok(Vec::new()));
    }

    #[test]
    fn damaged_files_fail_whole() {
        let good = build(&[
            (1, 0, ".", DIR),
            (2, 1, "Foo.app", DIR),
            (3, 2, "Contents", DIR),
            (4, 3, "Info.plist", FILE),
        ]);
        assert!(bundles(b"not a bom").is_err());
        assert!(bundles(&good[..good.len() / 2]).is_err());
        let mut wrong_version = good.clone();
        wrong_version[11] = 2;
        assert!(bundles(&wrong_version).is_err());
        let duplicate = build(&[(1, 0, ".", DIR), (1, 0, "x", DIR)]);
        assert_eq!(bundles(&duplicate), Err(BomError("two paths share an ID")));
    }

    proptest::proptest! {
        /// Damage anywhere, or a cut at any length, fails or reads; it never
        /// panics or loops.
        #[test]
        fn damage_never_panics(
            flips in proptest::collection::vec(
                (proptest::prelude::any::<proptest::sample::Index>(), proptest::prelude::any::<u8>()),
                0..32,
            ),
            cut in proptest::prelude::any::<proptest::sample::Index>(),
        ) {
            let mut bom = build(&[
                (1, 0, ".", DIR),
                (2, 1, "Foo.app", DIR),
                (3, 2, "Contents", DIR),
                (4, 3, "Info.plist", FILE),
                (5, 1, "Bar.app", DIR),
            ]);
            for (at, byte) in flips {
                let i = at.index(bom.len());
                bom[i] = byte;
            }
            let length = cut.index(bom.len() + 1);
            // Either outcome is fine; reaching here is the property.
            drop(bundles(&bom[..length]));
        }
    }

    #[test]
    fn a_chain_that_does_not_reach_the_top_names_nothing() {
        let bom = build(&[
            (2, 99, "Foo.app", DIR),
            (3, 2, "Contents", DIR),
            (4, 3, "Info.plist", FILE),
        ]);
        assert_eq!(bundles(&bom), Ok(Vec::new()));
    }
}
