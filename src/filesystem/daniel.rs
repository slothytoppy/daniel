use std::{
    ffi::c_int,
    num::NonZero,
    ops::ControlFlow,
    path::Path,
    time::{self, Duration},
};

use fuser::FileType;
static EPERM: i32 = 1;
static ENOENT: i32 = 2;
static ENOSYS: i32 = 38;
use tracing::{debug, error, info, instrument, warn};

use crate::{filesystem::EntryType, unchecked_inode};

use super::{DirEntry, DirList, Directory, FileAttribute, Inode, InodeMapper, file_types::File};

pub const ROOT_INODE: Inode = Inode::new(NonZero::new(1).unwrap());

#[derive(Debug, Default)]
pub struct Daniel {
    mapper: InodeMapper,
    list: DirList,
}

impl Daniel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, item: DirEntry) {
        let (parent, name, ino) = match &item {
            DirEntry::Directory(dir) => {
                let name = dir.name();
                let parent = dir.parent();
                (parent, name, dir.attr().inner().ino)
            }
            DirEntry::File(file) => {
                let name = file.name();
                let parent = file.parent();
                (parent, name, file.attr().inner().ino)
            }
        };

        let ino = unchecked_inode!(ino);
        self.mapper.insert(parent, name, ino);

        self.list
            .map_mut()
            .get_mut(&parent)
            .expect("failed to get dir")
            .directory_mut()
            .insert(
                ino,
                item.kind()
                    .try_into()
                    .expect("failed to convert file type into EntryType"),
            );

        self.list.map_mut().insert(ino, item);
    }

    pub fn create(
        &mut self,
        parent: Inode,
        path: impl AsRef<Path>,
        _mode: u16,
        perms: u16,
    ) -> FileAttribute {
        let inode = self.mapper.next_inode();
        self.push(DirEntry::File(File::new(
            path.as_ref().to_path_buf(),
            parent,
            inode,
            perms,
        )));

        self.list
            .map()
            .get(&inode)
            .expect("failed to get entry that was just pushed")
            .file()
            .attr()
    }

    fn mkdir(
        &mut self,
        parent: u64,
        name: &std::ffi::OsStr,
        mode: u32,
        umask: u32,
    ) -> FileAttribute {
        let inode = self.mapper.next_inode();
        self.push(DirEntry::Directory(Directory::new(
            unchecked_inode!(parent),
            name.into(),
            inode,
            0o755,
        )));

        self.list
            .map()
            .get(&inode)
            .expect("failed to get entry that was just pushed")
            .directory()
            .attr()
    }

    pub fn readdir(&self, ino: u64, _fh: u64, offset: u64) -> ControlFlow<(), &DirEntry> {
        let Some(entry) = self.list.map().get(&unchecked_inode!(ino)) else {
            return ControlFlow::Break(());
        };

        match entry {
            DirEntry::Directory(_directory) => {
                let idx = offset.max(1);
                let Some(entry) = self.list.map().get(&unchecked_inode!(idx)) else {
                    return ControlFlow::Break(());
                };

                ControlFlow::Continue(entry)
            }
            DirEntry::File(_file) => ControlFlow::Break(()),
        }
    }

    pub fn lookup(&mut self, parent: u64, name: &std::ffi::OsStr) -> Result<FileAttribute, ()> {
        for (ino, _) in self
            .list
            .map()
            .get(&unchecked_inode!(parent))
            .expect("failed to get dir")
            .directory()
            .entries()
            .iter()
        {
            let (path, attr) = match self.list.map().get(&ino).expect("invalid entry in dir") {
                DirEntry::Directory(directory) => (directory.name(), directory.attr()),
                DirEntry::File(file) => (file.name(), file.attr()),
            };

            if path == name {
                return Ok(attr);
            }
        }

        Err(())
    }

    pub fn access(&mut self, ino: u64, mask: i32) -> Result<(), ()> {
        match self.list.map().get(&unchecked_inode!(ino)) {
            Some(_) => Ok(()),
            None => Err(()),
        }
    }

    pub fn getattr(&mut self, ino: u64, fh: Option<u64>) -> &FileAttribute {
        self.list
            .map()
            .get(&unchecked_inode!(ino))
            .expect("failed to find ino in backing fs")
            .attr()
    }

    pub fn unlink(&mut self, parent: u64, name: &std::ffi::OsStr) -> Result<(), ()> {
        let ino = self
            .mapper
            .get_map(unchecked_inode!(parent), name)
            .expect("failed to find inode");
        self.list.map_mut().remove(ino);
        self.mapper.remove(unchecked_inode!(parent), name);

        Ok(())
    }
}

impl std::fmt::Display for Daniel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.mapper)?;
        write!(f, "{}", self.list)?;

        Ok(())
    }
}

impl fuser::Filesystem for Daniel {
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
        reply.created(
            &Duration::from_secs(1),
            &self
                .create(unchecked_inode!(parent), name, mode as u16, flags as u16)
                .inner(),
            0,
            0,
            flags as u32,
        );
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
        reply.entry(
            &Duration::from_secs(1),
            &self.mkdir(parent, name, mode, umask).inner(),
            0,
        );
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
        info!(?offset);
        if offset > 50 {
            panic!()
        }

        if offset == 0 {
            _ = reply.add(1, 1, FileType::Directory, ".");
        } else if offset == 1 {
            _ = reply.add(1, 2, FileType::Directory, "..");
        }

        let dir = self
            .list
            .map()
            .get(&unchecked_inode!(ino))
            .expect("failed to get dir in readdir");
        for (i, (ino, kind)) in dir
            .directory()
            .entries()
            .iter()
            .enumerate()
            .skip(offset as usize)
        {
            let Some(entry) = self.list.map().get(ino) else {
                reply.error(ENOENT);
                return;
            };

            let name = match entry {
                DirEntry::Directory(directory) => directory.name(),
                DirEntry::File(file) => file.name(),
            };

            info!(?offset, ?name);
            let kind = kind.clone();
            if reply.add(ino.into(), (i + 1) as i64, kind.into(), name) {
                error!("early return");
                break;
            }
        }

        reply.ok();
    }

    #[instrument(skip(self, _req, reply))]
    fn lookup(
        &mut self,
        _req: &fuser::Request<'_>,
        parent: u64,
        name: &std::ffi::OsStr,
        reply: fuser::ReplyEntry,
    ) {
        let Ok(attr) = self.lookup(parent, name) else {
            reply.error(ENOENT);
            return;
        };
        reply.entry(&Duration::from_secs(1), &attr.inner(), 0);
    }

    #[instrument(skip(self, _req, reply))]
    fn access(&mut self, _req: &fuser::Request<'_>, ino: u64, mask: i32, reply: fuser::ReplyEmpty) {
        let res = self.access(ino, mask);
        match res {
            Ok(()) => reply.ok(),
            Err(()) => reply.error(ENOENT),
        }
    }

    #[instrument(skip(self, _req, reply))]
    fn getattr(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: Option<u64>,
        reply: fuser::ReplyAttr,
    ) {
        reply.attr(&Duration::from_secs(1), &self.getattr(ino, fh).inner());
    }

    fn unlink(
        &mut self,
        _req: &fuser::Request<'_>,
        parent: u64,
        name: &std::ffi::OsStr,
        reply: fuser::ReplyEmpty,
    ) {
        let res = self.unlink(parent, name);
        match res {
            Ok(()) => reply.ok(),
            Err(()) => reply.error(ENOENT),
        }
    }

    fn init(
        &mut self,
        _req: &fuser::Request<'_>,
        _config: &mut fuser::KernelConfig,
    ) -> Result<(), c_int> {
        Ok(())
    }

    fn destroy(&mut self) {}

    fn forget(&mut self, _req: &fuser::Request<'_>, _ino: u64, _nlookup: u64) {}

    fn setattr(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        _mode: Option<u32>,
        _uid: Option<u32>,
        _gid: Option<u32>,
        size: Option<u64>,
        atime: Option<fuser::TimeOrNow>,
        mtime: Option<fuser::TimeOrNow>,
        ctime: Option<std::time::SystemTime>,
        fh: Option<u64>,
        _crtime: Option<std::time::SystemTime>,
        _chgtime: Option<std::time::SystemTime>,
        _bkuptime: Option<std::time::SystemTime>,
        flags: Option<u32>,
        reply: fuser::ReplyAttr,
    ) {
        let Some(entry) = self.list.map_mut().get_mut(&unchecked_inode!(ino)) else {
            reply.error(ENOENT);
            return;
        };
        let attr = entry.attr_mut().inner_mut();
        if let Some(size) = size {
            attr.size = size
        };

        if let Some(time) = atime {
            attr.atime = match time {
                fuser::TimeOrNow::SpecificTime(system_time) => system_time,
                fuser::TimeOrNow::Now => time::SystemTime::now(),
            }
        };

        if let Some(time) = mtime {
            attr.mtime = match time {
                fuser::TimeOrNow::SpecificTime(system_time) => system_time,
                fuser::TimeOrNow::Now => time::SystemTime::now(),
            }
        };

        if let Some(time) = ctime {
            attr.ctime = time
        };

        reply.attr(&Duration::from_secs(1), attr);
    }

    fn readlink(&mut self, _req: &fuser::Request<'_>, ino: u64, reply: fuser::ReplyData) {
        debug!("[Not Implemented] readlink(ino: {:#x?})", ino);
        reply.error(ENOSYS);
    }

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
        debug!(
            "[Not Implemented] mknod(parent: {:#x?}, name: {:?}, mode: {}, \
            umask: {:#x?}, rdev: {})",
            parent, name, mode, umask, rdev
        );
        reply.error(ENOSYS);
    }

    fn rmdir(
        &mut self,
        _req: &fuser::Request<'_>,
        parent: u64,
        name: &std::ffi::OsStr,
        reply: fuser::ReplyEmpty,
    ) {
        debug!(
            "[Not Implemented] rmdir(parent: {:#x?}, name: {:?})",
            parent, name,
        );
        reply.error(ENOSYS);
    }

    fn symlink(
        &mut self,
        _req: &fuser::Request<'_>,
        parent: u64,
        link_name: &std::ffi::OsStr,
        target: &Path,
        reply: fuser::ReplyEntry,
    ) {
        debug!(
            "[Not Implemented] symlink(parent: {:#x?}, link_name: {:?}, target: {:?})",
            parent, link_name, target,
        );
        reply.error(EPERM);
    }

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
        debug!(
            "[Not Implemented] rename(parent: {:#x?}, name: {:?}, newparent: {:#x?}, \
            newname: {:?}, flags: {})",
            parent, name, newparent, newname, flags,
        );
        reply.error(ENOSYS);
    }

    fn link(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        newparent: u64,
        newname: &std::ffi::OsStr,
        reply: fuser::ReplyEntry,
    ) {
        debug!(
            "[Not Implemented] link(ino: {:#x?}, newparent: {:#x?}, newname: {:?})",
            ino, newparent, newname
        );
        reply.error(EPERM);
    }

    fn open(&mut self, _req: &fuser::Request<'_>, _ino: u64, _flags: i32, reply: fuser::ReplyOpen) {
        reply.opened(0, 0);
    }

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
        reply.error(ENOSYS);
    }

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
        debug!(
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
        reply.error(ENOSYS);
    }

    fn flush(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        lock_owner: u64,
        reply: fuser::ReplyEmpty,
    ) {
        debug!(
            "[Not Implemented] flush(ino: {:#x?}, fh: {}, lock_owner: {:?})",
            ino, fh, lock_owner
        );
        reply.error(ENOSYS);
    }

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
        reply.ok();
    }

    fn fsync(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        datasync: bool,
        reply: fuser::ReplyEmpty,
    ) {
        debug!(
            "[Not Implemented] fsync(ino: {:#x?}, fh: {}, datasync: {})",
            ino, fh, datasync
        );
        reply.error(ENOSYS);
    }

    fn opendir(
        &mut self,
        _req: &fuser::Request<'_>,
        _ino: u64,
        _flags: i32,
        reply: fuser::ReplyOpen,
    ) {
        reply.opened(0, 0);
    }

    fn readdirplus(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        reply: fuser::ReplyDirectoryPlus,
    ) {
        debug!(
            "[Not Implemented] readdirplus(ino: {:#x?}, fh: {}, offset: {})",
            ino, fh, offset
        );
        reply.error(ENOSYS);
    }

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

    fn fsyncdir(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        datasync: bool,
        reply: fuser::ReplyEmpty,
    ) {
        debug!(
            "[Not Implemented] fsyncdir(ino: {:#x?}, fh: {}, datasync: {})",
            ino, fh, datasync
        );
        reply.error(ENOSYS);
    }

    fn statfs(&mut self, _req: &fuser::Request<'_>, _ino: u64, reply: fuser::ReplyStatfs) {
        reply.statfs(0, 0, 0, 0, 0, 512, 255, 0);
    }

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
        debug!(
            "[Not Implemented] setxattr(ino: {:#x?}, name: {:?}, flags: {:#x?}, position: {})",
            ino, name, flags, position
        );
        reply.error(ENOSYS);
    }

    fn getxattr(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        name: &std::ffi::OsStr,
        size: u32,
        reply: fuser::ReplyXattr,
    ) {
        debug!(
            "[Not Implemented] getxattr(ino: {:#x?}, name: {:?}, size: {})",
            ino, name, size
        );
        reply.error(ENOSYS);
    }

    fn listxattr(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        size: u32,
        reply: fuser::ReplyXattr,
    ) {
        debug!(
            "[Not Implemented] listxattr(ino: {:#x?}, size: {})",
            ino, size
        );
        reply.error(ENOSYS);
    }

    fn removexattr(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        name: &std::ffi::OsStr,
        reply: fuser::ReplyEmpty,
    ) {
        debug!(
            "[Not Implemented] removexattr(ino: {:#x?}, name: {:?})",
            ino, name
        );
        reply.error(ENOSYS);
    }

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
        debug!(
            "[Not Implemented] getlk(ino: {:#x?}, fh: {}, lock_owner: {}, start: {}, \
            end: {}, typ: {}, pid: {})",
            ino, fh, lock_owner, start, end, typ, pid
        );
        reply.error(ENOSYS);
    }

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
        debug!(
            "[Not Implemented] setlk(ino: {:#x?}, fh: {}, lock_owner: {}, start: {}, \
            end: {}, typ: {}, pid: {}, sleep: {})",
            ino, fh, lock_owner, start, end, typ, pid, sleep
        );
        reply.error(ENOSYS);
    }

    fn bmap(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        blocksize: u32,
        idx: u64,
        reply: fuser::ReplyBmap,
    ) {
        debug!(
            "[Not Implemented] bmap(ino: {:#x?}, blocksize: {}, idx: {})",
            ino, blocksize, idx,
        );
        reply.error(ENOSYS);
    }

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
        debug!(
            "[Not Implemented] ioctl(ino: {:#x?}, fh: {}, flags: {}, cmd: {}, \
            in_data.len(): {}, out_size: {})",
            ino,
            fh,
            flags,
            cmd,
            in_data.len(),
            out_size,
        );
        reply.error(ENOSYS);
    }

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
        debug!(
            "[Not Implemented] fallocate(ino: {:#x?}, fh: {}, offset: {}, \
            length: {}, mode: {})",
            ino, fh, offset, length, mode
        );
        reply.error(ENOSYS);
    }

    fn lseek(
        &mut self,
        _req: &fuser::Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        whence: i32,
        reply: fuser::ReplyLseek,
    ) {
        debug!(
            "[Not Implemented] lseek(ino: {:#x?}, fh: {}, offset: {}, whence: {})",
            ino, fh, offset, whence
        );
        reply.error(ENOSYS);
    }

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
        debug!(
            "[Not Implemented] copy_file_range(ino_in: {:#x?}, fh_in: {}, \
            offset_in: {}, ino_out: {:#x?}, fh_out: {}, offset_out: {}, \
            len: {}, flags: {})",
            ino_in, fh_in, offset_in, ino_out, fh_out, offset_out, len, flags
        );
        reply.error(ENOSYS);
    }
}

#[cfg(test)]
mod test {
    use std::ops::ControlFlow;

    use tracing::{info, instrument, level_filters::LevelFilter};
    use tracing_subscriber::{fmt::format::FmtSpan, util::SubscriberInitExt};

    use crate::{
        filesystem::{DirEntry, Directory, EntryType, File},
        unchecked_inode,
    };

    use super::{Daniel, ROOT_INODE};

    fn init() {
        let _ = tracing_subscriber::FmtSubscriber::builder()
            .with_ansi(true)
            .with_max_level(LevelFilter::INFO)
            .with_span_events(FmtSpan::ACTIVE)
            .finish()
            .try_init();
    }

    #[test]
    #[instrument]
    pub fn default() {
        init();
        let fs = Daniel::default();

        assert_eq!(fs.mapper.map().len(), 1);
        assert_eq!(fs.list.map().len(), 1);

        info!(%fs);
    }

    #[test]
    #[instrument]
    pub fn empty_dir() {
        init();

        let mut fs = Daniel::default();
        fs.push(DirEntry::Directory(Directory::new(
            ROOT_INODE,
            "foo".into(),
            unchecked_inode!(2),
            0o755,
        )));

        assert_eq!(fs.mapper.map().len(), 2);
        assert_eq!(fs.list.map().len(), 2);

        info!(%fs);
    }

    #[test]
    #[instrument]
    pub fn filled_dir() {
        init();

        let _s = tracing::info_span!("filled_dir");
        let _s = _s.entered();

        let mut fs = Daniel::default();

        fs.push(DirEntry::Directory(Directory::new(
            ROOT_INODE,
            "foo".into(),
            unchecked_inode!(2),
            0o755,
        )));

        let map = &fs.mapper;
        let list = &fs.list;

        info!(%map);
        info!(%list);

        let entry = fs
            .list
            .map_mut()
            .get_mut(&unchecked_inode!(2))
            .unwrap()
            .directory_mut();

        entry.insert(unchecked_inode!(3), EntryType::File);

        fs.push(DirEntry::File(File::new(
            "bar".into(),
            unchecked_inode!(2),
            unchecked_inode!(3),
            0o655,
        )));

        assert_eq!(fs.mapper.map().len(), 3);
        assert_eq!(fs.list.map().len(), 3);

        info!(%fs);
    }

    #[test]
    #[instrument]
    fn readdir() {
        init();

        let _s = tracing::info_span!("readdir");
        let _s = _s.entered();

        let mut fs = Daniel::new();

        fs.push(DirEntry::File(File::new(
            "foo".into(),
            ROOT_INODE,
            unchecked_inode!(2),
            0o655,
        )));

        let entry = fs.readdir(ROOT_INODE.into(), 0, 0);
        match entry {
            ControlFlow::Continue(entry) => match entry {
                DirEntry::Directory(directory) => {
                    let first = directory.entries().values().next().unwrap();
                    assert_eq!(first, &EntryType::File);
                }
                DirEntry::File(_file) => panic!("expected ROOT dir found file"),
            },
            ControlFlow::Break(_) => panic!(),
        }
    }
}
