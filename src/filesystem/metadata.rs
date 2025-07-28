#![allow(dead_code)]

use std::{
    collections::BTreeMap,
    num::NonZeroU64,
    ops::Add,
    path::{Path, PathBuf},
    time::SystemTime,
};

use fuser::{FileAttr, FileType};
use tracing::info;

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Debug, Hash)]
pub struct Inode(NonZeroU64);

impl Inode {
    pub fn new(ino: NonZeroU64) -> Self {
        Self(ino)
    }
}

impl TryFrom<u64> for Inode {
    type Error = ();
    fn try_from(value: u64) -> Result<Self, Self::Error> {
        if value == 0 {
            return Err(());
        }

        Ok(Inode::new(value.try_into().unwrap()))
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

#[derive(Debug)]
pub struct FileAttribute(FileAttr);

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

    pub fn inner(&self) -> &FileAttr {
        &self.0
    }

    pub fn inner_mut(&mut self) -> &mut FileAttr {
        &mut self.0
    }
}

#[derive(Debug)]
pub struct InodeList {
    inodes: BTreeMap<PathBuf, Inode>,
}

impl Default for InodeList {
    fn default() -> Self {
        let mut inodes = BTreeMap::new();

        inodes.insert("/".into(), Inode::new(1.try_into().unwrap()));

        Self { inodes }
    }
}

impl InodeList {
    fn contains_key(&self, path: PathBuf) -> bool {
        self.inodes.contains_key(&path)
    }

    fn contains_inode(&self, inode: Inode) -> bool {
        for (_, ino) in self.inodes.iter() {
            if ino == &inode {
                return true;
            }
        }

        false
    }

    pub fn push(&mut self, path: impl Into<PathBuf>) -> Inode {
        let value = self
            .inodes
            .last_key_value()
            .expect("root node was somehow removed :3")
            .1
            .add(1.try_into().unwrap());
        let path = path.into();

        info!(?path, ?value);
        info!(?self);

        if !self.inodes.contains_key(&path) {
            return self.inodes.insert(path, value).unwrap_or(value);
        }

        value
    }

    pub fn inner(&self) -> &BTreeMap<PathBuf, Inode> {
        &self.inodes
    }

    pub fn inner_mut(&mut self) -> &mut BTreeMap<PathBuf, Inode> {
        &mut self.inodes
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
            Inode::new(1.try_into().unwrap()),
            FileAttribute::new(1, FileType::Directory, 0o755),
        );

        Attr { attrs: map }
    }

    pub fn push(&mut self, ino: Inode, perms: u16, kind: FileType) -> &FileAttribute {
        self.attrs
            .insert(ino, FileAttribute::new(ino.0.get(), kind, perms));

        self.attrs.get(&ino).unwrap()
    }

    pub fn entries(&self) -> &BTreeMap<Inode, FileAttribute> {
        &self.attrs
    }

    pub fn entries_mut(&mut self) -> &mut BTreeMap<Inode, FileAttribute> {
        &mut self.attrs
    }
}

#[derive(Debug)]
pub struct MetaData {
    inodes: InodeList,
    attrs: Attr,
}

impl Default for MetaData {
    fn default() -> Self {
        let mut inodes = InodeList::default();
        Self {
            inodes,
            attrs: Attr::new(),
        }
    }
}

impl MetaData {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, path: &Path, kind: FileType, perms: u16) -> Inode {
        if !self.inodes.contains_key(path.to_path_buf()) {
            let entry = self
                .inodes
                .inodes
                .last_entry()
                .unwrap()
                .get()
                .add(1.try_into().unwrap());
            info!(?entry);
            let ino = self.inodes.push(path);
            self.attrs.push(entry, perms, kind);
            return ino;
        }

        *self.inodes.inodes.get(&path.to_path_buf()).unwrap()
    }

    pub fn inodes(&self) -> &InodeList {
        &self.inodes
    }

    pub fn attributes(&self) -> &Attr {
        &self.attrs
    }
}
