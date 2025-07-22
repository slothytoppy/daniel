use fuse::FileAttr;
use fuse::{FileType, ReplyAttr, ReplyDirectory, ReplyEntry, Request};
use libc::{EINVAL, ENOENT};
use std::collections::BTreeMap;
use std::env;
use std::ffi::OsStr;
use time::Timespec;

pub struct File<'a> {
    data: Vec<u8>,
    name: &'a str,
}

#[derive(Default)]
pub struct MetaData {
    pub attrs: BTreeMap<u64, FileAttr>,
    pub inodes: BTreeMap<String, u64>,
}

impl MetaData {
    fn new() -> Self {
        let time = time::get_time();
        let attr = FileAttr {
            ino: 1,
            size: 0,
            blocks: 0,
            atime: time,
            mtime: time,
            ctime: time,
            crtime: time,
            kind: FileType::Directory,
            perm: 0o755,
            nlink: 0,
            uid: 0,
            gid: 0,
            rdev: 0,
            flags: 0,
        };
        let mut attrs = BTreeMap::new();
        let mut inodes = BTreeMap::new();
        attrs.insert(1, attr);

        inodes.insert("/".to_string(), 1);

        Self { attrs, inodes }
    }

    fn push(&mut self, name: String, size: usize) {
        // this gets called after init so its always safe as theres at least one entry
        let pair = self.inodes.last_key_value().unwrap();
        let time = time::get_time();
        let attrs = FileAttr {
            ino: *pair.1 + 1,
            size: size as u64,
            blocks: 0,
            atime: time,
            mtime: time,
            ctime: time,
            crtime: time,
            kind: FileType::RegularFile,
            perm: 0o644,
            nlink: 0,
            uid: 0,
            gid: 0,
            rdev: 0,
            flags: 0,
        };
        self.attrs.insert(attrs.ino, attrs);

        self.inodes.insert(name.clone(), attrs.ino);
    }
}

impl<'a> File<'a> {
    pub fn new(name: &'a str) -> Self {
        Self { data: vec![], name }
    }
}

struct Daniel<'a> {
    data: Vec<File<'a>>,
    metadata: MetaData,
}

impl<'a> Daniel<'a> {
    fn new() -> Daniel<'a> {
        let mut metadata = MetaData::new();
        metadata.push("slothy".to_string(), 0);
        let file = vec![File::new("slothy"), File::new("slothy")];
        Daniel {
            data: file,
            metadata,
        }
        // let mut attrs = BTreeMap::new();
        // let mut inodes = BTreeMap::new();
        // let ts = time::now().to_timespec();
        // let attr = FileAttr {
        //     ino: 1,
        //     size: 0,
        //     blocks: 0,
        //     atime: ts,
        //     mtime: ts,
        //     ctime: ts,
        //     crtime: ts,
        //     kind: FileType::Directory,
        //     perm: 0o755,
        //     nlink: 0,
        //     uid: 0,
        //     gid: 0,
        //     rdev: 0,
        //     flags: 0,
        // };
        // attrs.insert(1, attr);
        // inodes.insert("/".to_string(), 1);
        // let attr = FileAttr {
        //     ino: 2,
        //     size: 0,
        //     blocks: 0,
        //     atime: ts,
        //     mtime: ts,
        //     ctime: ts,
        //     crtime: ts,
        //     kind: FileType::RegularFile,
        //     perm: 0o644,
        //     nlink: 0,
        //     uid: 0,
        //     gid: 0,
        //     rdev: 0,
        //     flags: 0,
        // };
        // attrs.insert(attr.ino, attr);
        // inodes.insert("slothy".to_string(), attr.ino);
        // Filesystem {
        //     data: vec![],
        //     attrs,
        //     inodes,
        // }
    }

    fn push(&mut self, file: File<'a>) {
        self.metadata.push(file.name.to_string(), file.data.len());
        self.data.push(file);
    }
}

impl<'a> fuse::Filesystem for Daniel<'a> {
    fn read(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        size: u32,
        reply: fuse::ReplyData,
    ) {
        println!("read:");
        println!("{offset} {size}");
        if ino == 1 {
            return;
        }
        match self.metadata.attrs.get(&ino) {
            Some(attr) => match self.data.get((attr.ino - 1) as usize) {
                Some(file) => {
                    if offset < file.data.len() as i64 || size >= file.data.len() as u32 {
                        let bytes = &file.data[offset as usize..file.data.len()];
                        reply.data(bytes);
                    } else {
                        let bytes = &file.data[offset as usize..size as usize];
                        println!("{bytes:?}");
                        reply.data(bytes);
                    }
                }
                None => {
                    reply.error(ENOENT);
                }
            },
            None => reply.error(ENOENT),
        };
    }
    fn getattr(&mut self, _req: &Request, ino: u64, reply: ReplyAttr) {
        println!("getattr(ino={ino})");
        match self.metadata.attrs.get(&ino) {
            Some(attr) => {
                let ttl = Timespec::new(1, 0);
                reply.attr(&ttl, attr);
            }
            None => reply.error(ENOENT),
        };
    }

    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        println!("lookup(parent={}, name={})", parent, name.to_str().unwrap());
        let inode = match self.metadata.inodes.get(name.to_str().unwrap()) {
            Some(inode) => inode,
            None => {
                reply.error(ENOENT);
                return;
            }
        };
        match self.metadata.attrs.get(inode) {
            Some(attr) => {
                let ttl = Timespec::new(1, 0);
                reply.entry(&ttl, attr, 0);
            }
            None => reply.error(ENOENT),
        };
    }

    fn readdir(
        &mut self,
        _req: &Request,
        ino: u64,
        fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        println!("readdir(ino={ino}, fh={fh}, offset={offset})");
        if ino == 1 {
            if offset == 0 {
                reply.add(1, 0, FileType::Directory, ".");
                reply.add(1, 1, FileType::Directory, "..");
                for (key, &inode) in &self.metadata.inodes {
                    if inode == 1 {
                        continue;
                    }
                    let offset = inode as i64; // hack
                    println!("\tkey={key}, inode={inode}, offset={offset}");
                    reply.add(inode, offset, FileType::RegularFile, key);
                }
            }
            reply.ok();
        } else {
            reply.error(ENOENT);
        }
    }
}

fn main() {
    let mut fs = Daniel::new();
    let mut file = File::new("slothy");
    file.data.push(b'h');
    file.data.push(b'e');
    file.data.push(b'l');
    file.data.push(b'l');
    file.data.push(b'o');
    fs.push(file);
    let mountpoint = match env::args().nth(1) {
        Some(path) => path,
        None => {
            println!("Usage: {} <MOUNTPOINT>", env::args().next().unwrap());
            return;
        }
    };

    fuse::mount(fs, &mountpoint, &[]).expect("Couldn't mount filesystem")
    // let session =
    //     unsafe { fuse::spawn_mount(fs, &mountpoint, &[]).expect("Couldn't mount filesystem") };
    //
    // let thread = session.guard.thread();
    //
    // sleep(Duration::new(1, 0));
    // let files = std::fs::read_dir(&mountpoint).expect("failed to read dir");
    // for file in files {
    //     println!("{:?}", file.unwrap());
    // }
    //
    // _ = std::process::Command::new("umount").arg(&mountpoint).exec();
}
