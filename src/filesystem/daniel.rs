use std::{
    collections::BTreeMap,
    num::NonZeroU64,
    path::{Path, PathBuf},
    time::{self, Duration, SystemTime},
};

use fuser::FileType;
use libc::{EEXIST, ENOENT, ENOSYS};
use tracing::{error, info, instrument, warn};

use super::{Attr, DirEntry, Directory, File, FileAttribute, Inode, InodeList, MetaData};

#[derive(Default, Debug)]
pub struct DirEntries {
    entries: BTreeMap<Inode, DirEntry>,
}

impl DirEntries {
    pub fn insert(&mut self, inode: Inode, entry: DirEntry) {
        self.entries.insert(inode, entry);
    }
}

#[derive(Debug)]
pub struct Root {
    attr: FileAttribute,
    inodes: InodeList,
    entries: DirEntries,
    attributes: Attr,
}

impl Default for Root {
    fn default() -> Self {
        Self {
            attr: FileAttribute::new(1, FileType::Directory, 0o755),
            inodes: InodeList::default(),
            entries: DirEntries::default(),
            attributes: Attr::new(),
        }
    }
}

impl Root {
    pub fn entries(&self) -> &DirEntries {
        &self.entries
    }

    pub fn entries_mut(&mut self) -> &mut DirEntries {
        &mut self.entries
    }

    pub fn inodes(&self) -> &InodeList {
        &self.inodes
    }

    pub fn attributes(&self) -> &FileAttribute {
        &self.attr
    }

    pub fn push(
        &mut self,
        parent: Option<Inode>,
        path: impl AsRef<Path>,
        kind: FileType,
        perms: u16,
    ) -> (Inode, &FileAttribute) {
        let path = path.as_ref();
        let ino = self.inodes.push(path);
        let attr = self.attributes.push(ino, perms, kind);
        match kind {
            FileType::Directory => {
                self.entries.insert(
                    ino,
                    DirEntry::Directory(Directory::new(parent.unwrap(), Some(path.into()))),
                );
            }
            FileType::RegularFile => {
                self.entries
                    .insert(ino, DirEntry::File(File::new(parent.unwrap(), path)));
            }

            _ => todo!(),
        }

        (ino, attr)
    }
}

#[derive(Debug, Default)]
pub struct Daniel {
    metadata: MetaData,
    root: Root,
    dir_entries: BTreeMap<Inode, DirEntry>,
}

impl Daniel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn root(&self) -> &Root {
        &self.root
    }
}

impl fuser::Filesystem for Daniel {
    #[instrument(skip(self, _req))]
    fn init(
        &mut self,
        _req: &fuser::Request<'_>,
        _config: &mut fuser::KernelConfig,
    ) -> Result<(), libc::c_int> {
        Ok(())
    }

    #[instrument(skip(self))]
    fn destroy(&mut self) {}

    #[instrument(skip(self, reply, _req))]
    fn lookup(
        &mut self,
        _req: &fuser::Request,
        parent: u64,
        name: &std::ffi::OsStr,
        reply: fuser::ReplyEntry,
    ) {
        match parent {
            1 => {
                let Some(ino) = self
                    .root
                    .inodes
                    .inner()
                    .get(&PathBuf::from(name.to_str().unwrap()))
                else {
                    reply.error(ENOENT);
                    return;
                };

                let ttl = Duration::from_secs(1);
                let attr = self
                    .root
                    .attributes
                    .entries()
                    .get(ino)
                    .expect("inode from attribute list does not exist");

                reply.entry(&ttl, attr.inner(), 0);
            }

            _ => {
                todo!("unimplemented non parent inode lookup")
            }
        }
    }

    #[instrument(skip(self, _req))]
    fn forget(&mut self, _req: &fuser::Request<'_>, _ino: u64, _nlookup: u64) {}

    #[instrument(skip(self, _req, reply))]
    fn getattr(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: Option<u64>,
        reply: fuser::ReplyAttr,
    ) {
        let attr = self
            .metadata
            .attributes()
            .entries()
            .get(&Inode::new(NonZeroU64::new(ino).unwrap()))
            .unwrap();

        let ttl = Duration::new(1, 0);
        reply.attr(&ttl, attr.inner());
    }

    #[instrument(skip_all)]
    fn setattr(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        _mode: Option<u32>,
        _uid: Option<u32>,
        _gid: Option<u32>,
        _size: Option<u64>,
        atime: Option<fuser::TimeOrNow>,
        mtime: Option<fuser::TimeOrNow>,
        ctime: Option<time::SystemTime>,
        _fh: Option<u64>,
        _crtime: Option<time::SystemTime>,
        _chgtime: Option<time::SystemTime>,
        _bkuptime: Option<time::SystemTime>,
        _flags: Option<u32>,
        reply: fuser::ReplyAttr,
    ) {
        let attr = self
            .root
            .attributes
            .entries_mut()
            .get_mut(&ino.try_into().unwrap())
            .expect("failed to retrieve attributes for setattr");
        if let Some(atime) = atime {
            match atime {
                fuser::TimeOrNow::SpecificTime(system_time) => {
                    attr.inner_mut().atime = system_time;
                }
                fuser::TimeOrNow::Now => {
                    attr.inner_mut().atime = SystemTime::now();
                }
            }
        }

        if let Some(atime) = ctime {
            attr.inner_mut().ctime = atime;
        }

        if let Some(atime) = mtime {
            match atime {
                fuser::TimeOrNow::SpecificTime(system_time) => {
                    attr.inner_mut().mtime = system_time;
                }
                fuser::TimeOrNow::Now => {
                    attr.inner_mut().mtime = SystemTime::now();
                }
            }
        }

        reply.attr(&Duration::from_secs(1), attr.inner());
    }

    #[instrument(skip_all)]
    fn readlink(&mut self, _req: &fuser::Request<'_>, ino: u64, reply: fuser::ReplyData) {
        info!("[Not Implemented] readlink(ino: {:#x?})", ino);
        reply.error(libc::ENOSYS);
    }

    #[instrument(skip_all)]
    fn mknod(
        &mut self,
        _req: &fuser::Request<'_>,
        parent: u64,
        name: &std::ffi::OsStr,
        mode: u32,
        umask: u32,
        rdev: u32,
        reply: fuser::ReplyEntry,
    ) {
        info!(
            "[Not Implemented] mknod(parent: {:#x?}, name: {:?}, mode: {}, \
            umask: {:#x?}, rdev: {})",
            parent, name, mode, umask, rdev
        );
        reply.error(libc::ENOSYS);
    }

    #[instrument(skip(self, _req, reply))]
    fn mkdir(
        &mut self,
        _req: &fuser::Request<'_>,
        parent: u64,
        name: &std::ffi::OsStr,
        mode: u32,
        umask: u32,
        reply: fuser::ReplyEntry,
    ) {
        match self.root.inodes.inner().get(&PathBuf::from(name)) {
            Some(_ino) => {
                reply.error(EEXIST);
            }

            None => {
                let ino = self.root.push(
                    Some(parent.try_into().unwrap()),
                    Path::new(name),
                    FileType::Directory,
                    0o755,
                );

                reply.entry(&time::Duration::new(1, 0), ino.1.inner(), 0);
            }
        }
    }

    #[instrument(skip_all)]
    fn unlink(
        &mut self,
        _req: &fuser::Request<'_>,
        parent: u64,
        name: &std::ffi::OsStr,
        reply: fuser::ReplyEmpty,
    ) {
        info!(
            "[Not Implemented] unlink(parent: {:#x?}, name: {:?})",
            parent, name,
        );
        reply.error(libc::ENOSYS);
    }

    #[instrument(skip_all)]
    fn rmdir(
        &mut self,
        _req: &fuser::Request<'_>,
        parent: u64,
        name: &std::ffi::OsStr,
        reply: fuser::ReplyEmpty,
    ) {
        info!(
            "[Not Implemented] rmdir(parent: {:#x?}, name: {:?})",
            parent, name,
        );
        reply.error(libc::ENOSYS);
    }

    #[instrument(skip_all)]
    fn symlink(
        &mut self,
        _req: &fuser::Request<'_>,
        parent: u64,
        link_name: &std::ffi::OsStr,
        target: &Path,
        reply: fuser::ReplyEntry,
    ) {
        info!(
            "[Not Implemented] symlink(parent: {:#x?}, link_name: {:?}, target: {:?})",
            parent, link_name, target,
        );
        reply.error(libc::EPERM);
    }

    #[instrument(skip_all)]
    fn rename(
        &mut self,
        _req: &fuser::Request<'_>,
        parent: u64,
        name: &std::ffi::OsStr,
        newparent: u64,
        newname: &std::ffi::OsStr,
        flags: u32,
        reply: fuser::ReplyEmpty,
    ) {
        info!(
            "[Not Implemented] rename(parent: {:#x?}, name: {:?}, newparent: {:#x?}, \
            newname: {:?}, flags: {})",
            parent, name, newparent, newname, flags,
        );
        reply.error(libc::ENOSYS);
    }

    #[instrument(skip_all)]
    fn link(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        newparent: u64,
        newname: &std::ffi::OsStr,
        reply: fuser::ReplyEntry,
    ) {
        info!(
            "[Not Implemented] link(ino: {:#x?}, newparent: {:#x?}, newname: {:?})",
            ino, newparent, newname
        );
        reply.error(libc::EPERM);
    }

    #[instrument(skip(self, _req, reply))]
    fn open(&mut self, _req: &fuser::Request<'_>, ino: u64, _flags: i32, reply: fuser::ReplyOpen) {
        reply.opened(2, _flags as u32);
    }

    #[instrument(skip_all)]
    fn read(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        size: u32,
        flags: i32,
        lock_owner: Option<u64>,
        reply: fuser::ReplyData,
    ) {
        warn!(
            "[Not Implemented] read(ino: {:#x?}, fh: {}, offset: {}, size: {}, \
            flags: {:#x?}, lock_owner: {:?})",
            ino, fh, offset, size, flags, lock_owner
        );
        reply.error(libc::ENOSYS);
    }

    #[instrument(skip_all)]
    fn write(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        data: &[u8],
        write_flags: u32,
        flags: i32,
        lock_owner: Option<u64>,
        reply: fuser::ReplyWrite,
    ) {
        info!(
            "[Not Implemented] write(ino: {:#x?}, fh: {}, offset: {}, data.len(): {}, \
            write_flags: {:#x?}, flags: {:#x?}, lock_owner: {:?})",
            ino,
            fh,
            offset,
            data.len(),
            write_flags,
            flags,
            lock_owner
        );
        reply.error(libc::ENOSYS);
    }

    #[instrument(skip(self, _req, reply))]
    fn flush(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        lock_owner: u64,
        reply: fuser::ReplyEmpty,
    ) {
        reply.error(ENOSYS);
    }

    #[instrument(skip(self, _req, reply))]
    fn release(
        &mut self,
        _req: &fuser::Request<'_>,
        _ino: u64,
        _fh: u64,
        _flags: i32,
        _lock_owner: Option<u64>,
        _flush: bool,
        reply: fuser::ReplyEmpty,
    ) {
        reply.error(ENOSYS);
    }

    #[instrument(skip(self, _req, reply))]
    fn fsync(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        datasync: bool,
        reply: fuser::ReplyEmpty,
    ) {
        reply.error(ENOSYS);
    }

    #[instrument(skip(self, _req, reply))]
    fn opendir(
        &mut self,
        _req: &fuser::Request<'_>,
        _ino: u64,
        _flags: i32,
        reply: fuser::ReplyOpen,
    ) {
        reply.opened(1, _flags as u32);
    }

    #[instrument(skip(self, _req, reply))]
    fn readdir(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        mut reply: fuser::ReplyDirectory,
    ) {
        let dir = self
            .root
            .entries
            .entries
            .get(&ino.try_into().unwrap())
            .unwrap()
            .dirent();

        if offset == 0 {
            info!("adding .");
            reply.add(ino, 1, FileType::Directory, ".");
        }

        if offset == 1 {
            info!("adding ..");
            // first arg is parent inode
            reply.add(dir.parent().into(), 2, FileType::Directory, "..");
        }

        for (i, ino) in dir.entries().enumerate().skip(offset as usize) {
            let entry = self
                .root
                .entries
                .entries
                .get(ino)
                .expect("failed to retrieve entry");
            if reply.add(ino.into(), (i + 1) as i64, entry.kind(), entry.name()) {
                break;
            }
        }

        reply.ok();
    }

    #[instrument(skip(self, _req, reply))]
    fn readdirplus(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        reply: fuser::ReplyDirectoryPlus,
    ) {
        reply.error(ENOSYS);
    }

    #[instrument(skip(self, _req, reply))]
    fn releasedir(
        &mut self,
        _req: &fuser::Request<'_>,
        _ino: u64,
        _fh: u64,
        _flags: i32,
        reply: fuser::ReplyEmpty,
    ) {
        reply.ok();
    }

    #[instrument(skip(self, _req, reply))]
    fn fsyncdir(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        datasync: bool,
        reply: fuser::ReplyEmpty,
    ) {
        reply.error(ENOSYS);
    }

    #[instrument(skip(self, _req, reply))]
    fn statfs(&mut self, _req: &fuser::Request<'_>, _ino: u64, reply: fuser::ReplyStatfs) {
        reply.error(ENOSYS);
        // reply.statfs(0, 0, 0, 0, 0, 512, 255, 0);
    }

    #[instrument(skip(self, _req, reply))]
    fn setxattr(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        name: &std::ffi::OsStr,
        _value: &[u8],
        flags: i32,
        position: u32,
        reply: fuser::ReplyEmpty,
    ) {
        reply.error(ENOSYS);
    }

    #[instrument(skip(self, _req, reply))]
    fn getxattr(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        name: &std::ffi::OsStr,
        size: u32,
        reply: fuser::ReplyXattr,
    ) {
        reply.error(ENOSYS);
    }

    #[instrument(skip(self, _req, reply))]
    fn listxattr(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        size: u32,
        reply: fuser::ReplyXattr,
    ) {
        reply.error(ENOSYS);
    }

    #[instrument(skip_all)]
    fn removexattr(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        name: &std::ffi::OsStr,
        reply: fuser::ReplyEmpty,
    ) {
        info!(
            "[Not Implemented] removexattr(ino: {:#x?}, name: {:?})",
            ino, name
        );
        reply.error(libc::ENOSYS);
    }

    #[instrument(skip(self, _req, reply))]
    fn access(&mut self, _req: &fuser::Request<'_>, ino: u64, mask: i32, reply: fuser::ReplyEmpty) {
        reply.ok();
    }

    #[instrument(skip(self, _req, reply))]
    fn create(
        &mut self,
        _req: &fuser::Request<'_>,
        parent: u64,
        name: &std::ffi::OsStr,
        mode: u32,
        umask: u32,
        flags: i32,
        reply: fuser::ReplyCreate,
    ) {
        info!("creating file");
        let (_, attr) = self.root.push(
            Some(parent.try_into().unwrap()),
            name,
            FileType::RegularFile,
            0o655,
        );

        let ttl = Duration::new(1, 0);
        let inner = attr.inner();

        info!(?inner);
        reply.created(&ttl, inner, 0, 0, 0o655);
    }

    #[instrument(skip_all)]
    fn getlk(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        lock_owner: u64,
        start: u64,
        end: u64,
        typ: i32,
        pid: u32,
        reply: fuser::ReplyLock,
    ) {
        info!(
            "[Not Implemented] getlk(ino: {:#x?}, fh: {}, lock_owner: {}, start: {}, \
            end: {}, typ: {}, pid: {})",
            ino, fh, lock_owner, start, end, typ, pid
        );
        reply.error(libc::ENOSYS);
    }

    #[instrument(skip_all)]
    fn setlk(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        lock_owner: u64,
        start: u64,
        end: u64,
        typ: i32,
        pid: u32,
        sleep: bool,
        reply: fuser::ReplyEmpty,
    ) {
        info!(
            "[Not Implemented] setlk(ino: {:#x?}, fh: {}, lock_owner: {}, start: {}, \
            end: {}, typ: {}, pid: {}, sleep: {})",
            ino, fh, lock_owner, start, end, typ, pid, sleep
        );
        reply.error(libc::ENOSYS);
    }

    #[instrument(skip_all)]
    fn bmap(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        blocksize: u32,
        idx: u64,
        reply: fuser::ReplyBmap,
    ) {
        info!(
            "[Not Implemented] bmap(ino: {:#x?}, blocksize: {}, idx: {})",
            ino, blocksize, idx,
        );
        reply.error(libc::ENOSYS);
    }

    #[instrument(skip_all)]
    fn ioctl(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        flags: u32,
        cmd: u32,
        in_data: &[u8],
        out_size: u32,
        reply: fuser::ReplyIoctl,
    ) {
        info!(
            "[Not Implemented] ioctl(ino: {:#x?}, fh: {}, flags: {}, cmd: {}, \
            in_data.len(): {}, out_size: {})",
            ino,
            fh,
            flags,
            cmd,
            in_data.len(),
            out_size,
        );
        reply.error(libc::ENOSYS);
    }

    #[instrument(skip_all)]
    fn fallocate(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        length: i64,
        mode: i32,
        reply: fuser::ReplyEmpty,
    ) {
        info!(
            "[Not Implemented] fallocate(ino: {:#x?}, fh: {}, offset: {}, \
            length: {}, mode: {})",
            ino, fh, offset, length, mode
        );
        reply.error(libc::ENOSYS);
    }

    #[instrument(skip_all)]
    fn lseek(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        whence: i32,
        reply: fuser::ReplyLseek,
    ) {
        info!(
            "[Not Implemented] lseek(ino: {:#x?}, fh: {}, offset: {}, whence: {})",
            ino, fh, offset, whence
        );
        reply.error(libc::ENOSYS);
    }

    #[instrument(skip_all)]
    fn copy_file_range(
        &mut self,
        _req: &fuser::Request<'_>,
        ino_in: u64,
        fh_in: u64,
        offset_in: i64,
        ino_out: u64,
        fh_out: u64,
        offset_out: i64,
        len: u64,
        flags: u32,
        reply: fuser::ReplyWrite,
    ) {
        info!(
            "[Not Implemented] copy_file_range(ino_in: {:#x?}, fh_in: {}, \
            offset_in: {}, ino_out: {:#x?}, fh_out: {}, offset_out: {}, \
            len: {}, flags: {})",
            ino_in, fh_in, offset_in, ino_out, fh_out, offset_out, len, flags
        );
        reply.error(libc::ENOSYS);
    }
}
