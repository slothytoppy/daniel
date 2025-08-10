use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use fuser::{FileAttr, FileType};

use super::{FileAttribute, Inode, ROOT_INODE};

#[derive(Debug, Clone)]
pub enum DirEntry {
    Directory(Directory),
    File(File),
}

impl std::fmt::Display for DirEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DirEntry::Directory(directory) => write!(f, "{directory}")?,
            DirEntry::File(file) => write!(f, "{file}")?,
        };

        Ok(())
    }
}

impl DirEntry {
    pub fn kind(&self) -> FileType {
        match self {
            DirEntry::Directory(_) => FileType::Directory,
            DirEntry::File(_) => FileType::RegularFile,
        }
    }

    pub fn directory(&self) -> &Directory {
        match self {
            DirEntry::Directory(directory) => directory,
            DirEntry::File(_) => panic!(),
        }
    }

    pub fn directory_mut(&mut self) -> &mut Directory {
        match self {
            DirEntry::Directory(directory) => directory,
            DirEntry::File(_) => panic!(),
        }
    }

    pub fn file(&self) -> &File {
        match self {
            DirEntry::Directory(_) => panic!(),
            DirEntry::File(file) => file,
        }
    }

    pub fn file_mut(&mut self) -> &mut File {
        match self {
            DirEntry::Directory(_) => panic!(),
            DirEntry::File(file) => file,
        }
    }

    pub fn attr(&self) -> &FileAttribute {
        match self {
            DirEntry::Directory(dir) => &dir.attr,
            DirEntry::File(file) => &file.attr,
        }
    }

    pub fn attr_mut(&mut self) -> &mut FileAttribute {
        match self {
            DirEntry::Directory(dir) => &mut dir.attr,
            DirEntry::File(file) => &mut file.attr,
        }
    }
}

#[derive(Debug, Clone)]
pub struct File {
    parent: Inode,
    attr: FileAttribute,
    name: PathBuf,
}

impl File {
    pub fn new(name: PathBuf, parent: Inode, inode: Inode, perms: u16) -> Self {
        Self {
            name,
            parent,
            attr: FileAttribute::new(inode.into(), FileType::RegularFile, perms),
        }
    }

    pub fn parent(&self) -> Inode {
        self.parent
    }
}

impl File {
    pub fn name(&self) -> &Path {
        &self.name
    }

    pub fn attr(&self) -> FileAttribute {
        self.attr
    }
}

impl std::fmt::Display for File {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "name: {:?} ", self.name().display())?;
        write!(f, "parent: {:?} ", self.parent)?;
        let attr: FileAttr = self.attr.into();
        write!(
            f,
            "File attr: type: {:?} inode: {} perms: {:o} ",
            attr.kind, attr.ino, attr.perm
        )?;

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum EntryType {
    File,
    Directory,
}

impl From<EntryType> for FileType {
    fn from(value: EntryType) -> Self {
        match value {
            EntryType::File => FileType::RegularFile,
            EntryType::Directory => FileType::Directory,
        }
    }
}

impl TryFrom<FileType> for EntryType {
    type Error = ();
    fn try_from(value: FileType) -> Result<Self, Self::Error> {
        match value {
            FileType::Directory => Ok(EntryType::Directory),
            FileType::RegularFile => Ok(EntryType::File),

            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Directory {
    parent: Inode,
    name: PathBuf,
    attr: FileAttribute,

    entries: HashMap<Inode, EntryType>,
}

#[derive(Debug)]
pub enum DirectoryInodes {
    Dir(Inode),
    File(Inode),
}

impl Directory {
    pub fn new(parent: Inode, name: PathBuf, inode: Inode, perms: u16) -> Self {
        Self {
            parent,
            name,
            attr: FileAttribute::new(inode.into(), FileType::Directory, perms),

            entries: HashMap::default(),
        }
    }

    pub fn parent(&self) -> Inode {
        self.parent
    }

    pub fn name(&self) -> &Path {
        &self.name
    }

    pub fn push(&mut self, inode: Inode, entry: EntryType) {
        self.entries.insert(inode, entry);
    }

    pub fn insert(&mut self, inode: Inode, entry: EntryType) {
        self.entries.insert(inode, entry);
    }

    pub fn get(&self, inode: &Inode) -> Option<&EntryType> {
        self.entries.get(inode)
    }

    pub fn attr(&self) -> FileAttribute {
        self.attr
    }

    pub fn entries(&self) -> &HashMap<Inode, EntryType> {
        &self.entries
    }
}

impl std::fmt::Display for Directory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Dir name: {:?}, ", self.name.display())?;
        write!(f, "parent: {}, ", Into::<u64>::into(self.parent))?;
        let attr: FileAttr = self.attr.into();
        write!(
            f,
            "attr: type: {:?} inode: {} perms: {:o} ",
            attr.kind, attr.ino, attr.perm
        )?;

        if !self.entries.is_empty() {
            for ino in &self.entries {
                writeln!(f, "entry: {ino:?} ")?;
            }
        } else {
            writeln!(f, "entries: []")?;
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct DirList {
    map: HashMap<Inode, DirEntry>,
}

impl Default for DirList {
    fn default() -> Self {
        let mut map = HashMap::new();
        map.insert(
            ROOT_INODE,
            DirEntry::Directory(Directory::new(ROOT_INODE, "/".into(), ROOT_INODE, 0o755)),
        );
        Self { map }
    }
}

impl DirList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn map(&self) -> &HashMap<Inode, DirEntry> {
        &self.map
    }

    pub fn map_mut(&mut self) -> &mut HashMap<Inode, DirEntry> {
        &mut self.map
    }

    pub fn insert(&mut self, inode: Inode, entry: DirEntry) {
        self.map.insert(inode, entry);
    }
}

impl std::fmt::Display for DirList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (_ino, entry) in self.map().iter() {
            match entry {
                DirEntry::Directory(directory) => {
                    write!(f, "{directory}")?;
                }
                DirEntry::File(file) => write!(f, "{file}")?,
            }
        }

        Ok(())
    }
}
