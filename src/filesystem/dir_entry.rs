use std::{
    collections::{HashSet, hash_set::Iter},
    io::Write,
    path::PathBuf,
};

use fuser::FileType;

use super::{FileAttribute, Inode};

#[derive(Debug)]
pub struct Directory {
    parent: Inode,
    name: PathBuf,
    list: DirList,
}

#[derive(Debug, Default)]
pub struct DirList {
    entries: HashSet<Inode>,
}

impl DirList {
    pub fn push(&mut self, inode: Inode) {
        self.entries.insert(inode);
    }
}

impl Directory {
    pub fn new(parent: Inode, name: Option<PathBuf>) -> Self {
        Self {
            parent,
            name: name.unwrap_or_default(),
            list: DirList::default(),
        }
    }

    pub fn parent(&self) -> Inode {
        self.parent
    }

    pub fn push_entry(&mut self, entry: Inode) {
        self.list.push(entry);
    }

    pub fn entries(&self) -> Iter<Inode> {
        self.list.entries.iter()
    }

    pub fn name(&self) -> &PathBuf {
        &self.name
    }
}

#[derive(Debug)]
pub struct File {
    parent: Inode,
    name: PathBuf,
    data: Vec<u8>,
}

impl File {
    pub fn new(parent: Inode, name: impl Into<PathBuf>) -> Self {
        Self {
            parent,
            name: name.into(),
            data: vec![],
        }
    }

    pub fn parent(&self) -> Inode {
        self.parent
    }

    pub fn write(&mut self, bytes: impl AsRef<[u8]>) {
        _ = self.data.write(bytes.as_ref());
    }

    pub fn name(&self) -> &PathBuf {
        &self.name
    }
}

#[derive(Debug)]
pub enum DirEntry {
    Directory(Directory),
    File(File),
}

impl DirEntry {
    pub fn dirent(&self) -> &Directory {
        match self {
            DirEntry::Directory(directory) => directory,
            DirEntry::File(_file) => panic!("expected directory found file"),
        }
    }

    pub fn dirent_mut(&mut self) -> &mut Directory {
        match self {
            DirEntry::Directory(directory) => directory,
            DirEntry::File(_file) => panic!("expected directory found file"),
        }
    }

    pub fn name(&self) -> &PathBuf {
        match self {
            DirEntry::Directory(directory) => directory.name(),
            DirEntry::File(file) => file.name(),
        }
    }

    pub fn file(&self) -> &File {
        match self {
            DirEntry::Directory(_directory) => panic!("expected file found directory"),
            DirEntry::File(file) => file,
        }
    }

    pub fn kind(&self) -> FileType {
        match self {
            DirEntry::Directory(_directory) => FileType::Directory,
            DirEntry::File(_file) => FileType::RegularFile,
        }
    }
}
