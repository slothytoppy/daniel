#![allow(dead_code)]

use std::{
    collections::{BTreeMap, btree_map::Iter},
    num::NonZeroU64,
    ops::Add,
    path::{Path, PathBuf},
    time::SystemTime,
};

use fuser::{FileAttr, FileType};

use super::ROOT_INODE;

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Debug, Hash)]
pub struct Inode(NonZeroU64);

impl Inode {
    pub const fn new(ino: NonZeroU64) -> Self {
        Self(ino)
    }
}

impl TryFrom<u64> for Inode {
    type Error = ();

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        Ok(Self(NonZeroU64::new(value).unwrap()))
    }
}

impl From<Inode> for u64 {
    fn from(value: Inode) -> Self {
        value.0.get()
    }
}

impl From<&Inode> for u64 {
    fn from(value: &Inode) -> Self {
        value.0.get()
    }
}

impl Add<NonZeroU64> for Inode {
    type Output = Inode;

    fn add(self, rhs: NonZeroU64) -> Self::Output {
        Inode::new(self.0.saturating_add(rhs.into()))
    }
}

#[macro_export]
macro_rules! nonzero_u64 {
    ($val:expr) => {{
        use std::num::NonZeroU64;
        assert!($val != 0);
        NonZeroU64::new($val).unwrap()
    }};
}

#[macro_export]
macro_rules! unchecked_inode {
    ($val:expr) => {{
        assert!($val != 0);
        use super::Inode;
        use std::num::NonZeroU64;
        Inode::new(NonZeroU64::new($val).unwrap())
    }};
}

#[derive(Debug)]
pub struct InodeMapper {
    paths: BTreeMap<PathBuf, Inode>,
    map: BTreeMap<(Inode, PathBuf), Inode>,
    /// if inode is removed, it sets it to that inode, else its the last inode + 1
    next_inode: Inode,
}

impl Default for InodeMapper {
    fn default() -> Self {
        let mut map = BTreeMap::new();
        map.insert((ROOT_INODE, "/".into()), ROOT_INODE);
        let mut paths = BTreeMap::new();
        paths.insert("/".into(), ROOT_INODE);

        Self {
            paths,
            map,
            next_inode: unchecked_inode!(2),
        }
    }
}

impl InodeMapper {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn map(&self) -> Iter<(Inode, PathBuf), Inode> {
        self.map.iter()
    }

    pub fn insert(&mut self, parent: Inode, path: impl AsRef<Path>, inode: Inode) {
        self.map
            .insert((parent, path.as_ref().to_path_buf()), inode);

        self.paths.insert(path.as_ref().to_path_buf(), inode);

        self.next_inode = self.next_inode.add(nonzero_u64!(1));
    }

    pub fn remove(&mut self, parent: Inode, path: impl AsRef<Path>) {
        self.map.remove(&(parent, path.as_ref().to_path_buf()));
        self.paths.remove(&path.as_ref().to_path_buf());
    }

    pub fn get_map(&self, parent: Inode, path: impl AsRef<Path>) -> Option<&Inode> {
        self.map.get(&(parent, path.as_ref().to_path_buf()))
    }

    pub fn get_path(&self, path: impl AsRef<Path>) -> Option<&Inode> {
        self.paths.get(&path.as_ref().to_path_buf())
    }

    pub fn next_inode(&self) -> Inode {
        self.next_inode
    }
}

impl std::fmt::Display for InodeMapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "InodeMapper ")?;
        let mut iter = self.map.iter();
        let mut len = iter.len();

        loop {
            let sep = if len > 1 { "," } else { " " };
            match iter.next() {
                Some(((parent, path), inode)) => write!(
                    f,
                    "path: {:?} parent: {:?} inode: {:?}{sep} ",
                    path.display(),
                    parent,
                    inode
                )?,
                None => {
                    write!(f, " ")?;
                    break;
                }
            }
            len -= 1;
        }

        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FileAttribute(FileAttr);

impl From<FileAttribute> for FileAttr {
    fn from(value: FileAttribute) -> Self {
        value.0
    }
}

impl FileAttribute {
    pub fn new(ino: u64, kind: FileType, perm: u16) -> Self {
        let time = SystemTime::now();
        let attr = FileAttr {
            ino,
            size: 0,
            blocks: 0,
            atime: time,
            mtime: time,
            ctime: time,
            crtime: time,
            kind,
            perm,
            nlink: 0,
            uid: 0,
            gid: 0,
            rdev: 0,
            flags: 0,
            blksize: 0,
        };

        Self(attr)
    }

    pub fn inner(&self) -> FileAttr {
        self.0
    }

    pub fn inner_mut(&mut self) -> &mut FileAttr {
        &mut self.0
    }
}

#[derive(Default, Debug)]
pub struct Attr {
    attrs: BTreeMap<Inode, FileAttribute>,
}

impl Attr {
    pub fn new() -> Self {
        let mut map = BTreeMap::new();
        map.insert(
            ROOT_INODE,
            FileAttribute::new(1, FileType::Directory, 0o755),
        );

        Attr { attrs: map }
    }

    pub fn push(&mut self, ino: Inode, perms: u16, kind: FileType) -> &FileAttribute {
        self.attrs
            .entry(ino)
            .or_insert(FileAttribute::new(ino.0.get(), kind, perms))
    }

    pub fn entries(&self) -> &BTreeMap<Inode, FileAttribute> {
        &self.attrs
    }

    pub fn entries_mut(&mut self) -> &mut BTreeMap<Inode, FileAttribute> {
        &mut self.attrs
    }
}
