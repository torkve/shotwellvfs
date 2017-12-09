extern crate libc;
extern crate time;
extern crate fuse;
extern crate sqlite;
#[macro_use] extern crate log;
extern crate env_logger;

use std::path::Path;
use std::ffi::OsStr;
use time::Timespec;
use libc::ENOENT;
use fuse::{Filesystem, ReplyEntry, ReplyAttr, FileAttr, FileType};

const TTL: Timespec = Timespec { sec: 60, nsec: 0};
const NOTIME: Timespec = Timespec { sec: 1, nsec: 0};

const ROOT: u64 = 1;
const PHOTO: u64 = 1 << 51;
const VIDEO: u64 = 1 << 52;
const TAG: u64 = 1 << 53;
const EVENT: u64 = 1 << 54;

enum FileId {
    FileKind(u64),
    DirKind(u64),
    VideoKind(u64),
}

trait TextField {
    fn read_text(&self, i: usize) -> String;
}

impl<'l> TextField for sqlite::Statement<'l> {
    fn read_text(&self, i: usize) -> String {
        match self.read::<Vec<u8>>(i) {
            Err(_) => String::new(),
            Ok(x) => String::from_utf8(x).unwrap_or(String::new())
        }
    }
}


macro_rules! dir_attr(
    ( $($name:ident => $inode:ident),* ) => {
        $(
            const $name: FileAttr = FileAttr {
                ino: $inode,
                size: 0,
                blocks: 0,
                atime: NOTIME,
                mtime: NOTIME,
                ctime: NOTIME,
                crtime: NOTIME,
                kind: FileType::Directory,
                perm: 0o555,
                nlink: 1,
                uid: 0,
                gid: 0,
                rdev: 0,
                flags: 0,
            };
        )*
    }
);
dir_attr!(ROOT_ATTR => ROOT,
          PHOTO_ATTR => PHOTO,
          VIDEO_ATTR => VIDEO,
          TAG_ATTR => TAG,
          EVENT_ATTR => EVENT);

fn make_dirattr(inode: u64, ts: Timespec) -> FileAttr {
    FileAttr {
        ino: inode,
        size: 0,
        blocks: 0,
        atime: ts,
        mtime: ts,
        ctime: ts,
        crtime: ts,
        kind: FileType::Directory,
        perm: 0o555,
        nlink: 1,
        uid: 0,
        gid: 0,
        rdev: 0,
        flags: 0,
    }
}

fn make_fileattr(inode: u64, filesize: u64, ts: Timespec) -> FileAttr {
    FileAttr {
        ino: inode,
        size: filesize,
        blocks: 0,
        atime: ts,
        mtime: ts,
        ctime: ts,
        crtime: ts,
        kind: FileType::RegularFile,
        perm: 0o555,
        nlink: 1,
        uid: 0,
        gid: 0,
        rdev: 0,
        flags: 0,
    }
}

struct ShotwellVFS {
    conn: sqlite::Connection,
}

impl ShotwellVFS {
    fn new<T: AsRef<Path>>(path: T) -> Self {
        ShotwellVFS {
            conn: sqlite::open(path).unwrap(),
        }
    }

    fn extract_id(&self, filename: &OsStr) -> Option<FileId> {
        let filename = filename.to_str();
        match filename {
            Some(x) => match x.chars().next() {
                Some('[') => x.find(']').and_then(|end| x[1..end].parse::<u64>().map(|idx| FileId::DirKind(idx)).ok()),
                Some('(') => x.find(')').and_then(|end| x[1..end].parse::<u64>().map(|idx| FileId::FileKind(idx)).ok()),
                _ => None,
            },
            _ => None
        }
    }

    fn readdir_root(&mut self, mut reply: fuse::ReplyDirectory, offset: i64) {
        if offset != 0 {
            reply.error(ENOENT);
        } else {
            reply.add(ROOT, 0, FileType::Directory, ".");
            reply.add(ROOT, 1, FileType::Directory, "..");
            reply.add(PHOTO, 2, FileType::Directory, "photos");
            reply.add(VIDEO, 3, FileType::Directory, "videos");
            reply.add(TAG, 4, FileType::Directory, "tags");
            reply.add(EVENT, 5, FileType::Directory, "events");
            reply.ok()
        }
    }

    fn readdir_events(&mut self, mut reply: fuse::ReplyDirectory, offset: i64) {
        if offset < 0 {
            reply.error(ENOENT);
            return;
        }
        if offset == 0 {
            reply.add(EVENT, 0, FileType::Directory, ".");
            reply.add(ROOT, 1, FileType::Directory, "..");
        }
        let mut idx = offset + 2;

        let mut statement = self.conn.prepare("SELECT id, name, time_created FROM EventTable ORDER BY time_created ASC LIMIT ?, 100").unwrap();
        statement.bind(1, offset).unwrap();
        while let Ok(sqlite::State::Row) = statement.next() {
            let event_id = statement.read::<i64>(0).unwrap() as u64;
            let inode = event_id | EVENT;
            let name = statement.read_text(1);
            if !name.is_empty() {
                debug!("event id {} has utf name {:?}", event_id, name);
                reply.add(inode, idx, FileType::Directory, format!("[{}] {}", event_id, name));
                idx += 1;
            } else {
                let timestamp = time::at(time::Timespec{sec: statement.read::<i64>(2).unwrap(), nsec: 0});
                let tm = timestamp.strftime("%Y-%m-%d %H:%M").unwrap();
                debug!("event id {} event name is empty, using timestamp `{}`", event_id, tm);
                reply.add(inode, idx, FileType::Directory, format!("[{}] {}", event_id, tm));
                idx += 1;
            }
        };
        reply.ok();
    }

    fn readdir_tags(&mut self, mut reply: fuse::ReplyDirectory, offset: i64) {
        if offset < 0 {
            reply.error(ENOENT);
            return;
        }
        if offset == 0 {
            reply.add(EVENT, 0, FileType::Directory, ".");
            reply.add(ROOT, 1, FileType::Directory, "..");
        }
        let mut idx = offset + 2;

        let mut statement = self.conn.prepare("SELECT id, LTRIM(name, '/') as tname, time_created FROM TagTable WHERE INSTR(tname, '/') = 0 ORDER BY tname ASC LIMIT ?, 100").unwrap();
        statement.bind(1, offset).unwrap();
        while let Ok(sqlite::State::Row) = statement.next() {
            let tag_id = statement.read::<i64>(0).unwrap() as u64;
            let inode = tag_id | EVENT;
            let name = statement.read_text(1);
            if !name.is_empty() {
                let mut title = &name[..];
                if title.starts_with('/') {
                    title = &title[1..];
                }
                debug!("tag id {} has utf name {:?}", tag_id, title);
                reply.add(inode, idx, FileType::Directory, format!("[{}] {}", tag_id, title));
                idx += 1;
            }
        };
        reply.ok();
    }

    fn readdir_photos(&mut self, mut reply: fuse::ReplyDirectory, offset: i64) {
        if offset < 0 {
            reply.error(ENOENT);
            return;
        }
        if offset == 0 {
            reply.add(PHOTO, 0, FileType::Directory, ".");
            reply.add(ROOT, 1, FileType::Directory, "..");
        }
        let mut idx = offset + 2;
        let mut statement = self.conn.prepare("SELECT id, filename, timestamp, title FROM PhotoTable ORDER BY timestamp ASC, id ASC LIMIT ?, 100").unwrap();
        statement.bind(1, offset).unwrap();
        while let Ok(sqlite::State::Row) = statement.next() {
            let photo_id = statement.read::<i64>(0).unwrap() as u64;
            let inode = photo_id | PHOTO;
            let filename = statement.read_text(1);
            let extension = filename.rfind('.').map(|x| &filename[x+1..]).unwrap_or("");
            let title = statement.read_text(3);
            if !title.is_empty() {
                debug!("photo id {} has utf name {:?}", photo_id, title);
                reply.add(inode, idx, FileType::RegularFile, format!("({}) {}.{}", photo_id, title, extension));
                idx += 1;
            } else {
                let timestamp = time::at(time::Timespec{sec: statement.read::<i64>(2).unwrap(), nsec: 0});
                let tm = timestamp.strftime("%Y-%m-%d %H:%M").unwrap();
                debug!("photo id {} title is empty, using timestamp `{}`", photo_id, tm);
                reply.add(inode, idx, FileType::RegularFile, format!("({}) {}.{}", photo_id, tm, extension));
                idx += 1;
            }
        };
        reply.ok();
    }

    fn readdir_videos(&mut self, mut reply: fuse::ReplyDirectory, offset: i64) {
        if offset < 0 {
            reply.error(ENOENT);
            return;
        }
        if offset == 0 {
            reply.add(VIDEO, 0, FileType::Directory, ".");
            reply.add(ROOT, 1, FileType::Directory, "..");
        }
        let mut idx = offset + 2;
        let mut statement = self.conn.prepare("SELECT id, filename, timestamp, title FROM VideoTable ORDER BY timestamp ASC LIMIT ?, 100").unwrap();
        statement.bind(1, offset).unwrap();
        while let Ok(sqlite::State::Row) = statement.next() {
            let video_id = statement.read::<i64>(0).unwrap() as u64;
            let inode = video_id | VIDEO;
            let filename = statement.read_text(1);
            let extension = filename.rfind('.').map(|x| &filename[x+1..]).unwrap_or("");
            let title = statement.read_text(3);
            if !title.is_empty() {
                debug!("video id {} has utf name {:?}", video_id, title);
                reply.add(inode, idx, FileType::RegularFile, format!("({}) {}.{}", video_id, title, extension));
                idx += 1;
            } else {
                let timestamp = time::at(time::Timespec{sec: statement.read::<i64>(2).unwrap(), nsec: 0});
                let tm = timestamp.strftime("%Y-%m-%d %H:%M").unwrap();
                debug!("video id {} title is empty, using timestamp `{}`", video_id, tm);
                reply.add(inode, idx, FileType::RegularFile, format!("({}) {}.{}", video_id, tm, extension));
                idx += 1;
            }
        };
        reply.ok();
    }

    fn lookup_root(&mut self, name: &OsStr, reply: ReplyEntry) {
        match name.to_str() {
            Some("/") => reply.entry(&TTL, &ROOT_ATTR, 0),
            Some("photos") => reply.entry(&TTL, &PHOTO_ATTR, 0),
            Some("videos") => reply.entry(&TTL, &VIDEO_ATTR, 0),
            Some("tags") => reply.entry(&TTL, &TAG_ATTR, 0),
            Some("events") => reply.entry(&TTL, &EVENT_ATTR, 0),
            _ => reply.error(ENOENT),
        }
    }

    fn lookup_event(&mut self, name: &OsStr, reply: ReplyEntry) {
        if let Some(FileId::DirKind(id)) = self.extract_id(name) {
            let mut statement = self.conn.prepare("SELECT time_created FROM EventTable WHERE id = ?").unwrap();
            statement.bind(1, id as i64).unwrap();
            if let Ok(sqlite::State::Row) = statement.next() {
                let timestamp = time::Timespec{sec: statement.read::<i64>(0).unwrap(), nsec: 0};
                reply.entry(&TTL, &make_dirattr(EVENT | id, timestamp), 0);
                return;
            }
        }
        reply.error(ENOENT);
    }

    fn lookup_tag(&mut self, name: &OsStr, reply: ReplyEntry) {
        if let Some(FileId::DirKind(id)) = self.extract_id(name) {
            let mut statement = self.conn.prepare("SELECT time_created FROM TagTable WHERE id = ?").unwrap();
            statement.bind(1, id as i64).unwrap();
            if let Ok(sqlite::State::Row) = statement.next() {
                let timestamp = time::Timespec{sec: statement.read::<i64>(0).unwrap(), nsec: 0};
                reply.entry(&TTL, &make_dirattr(TAG | id, timestamp), 0);
                return;
            }
        }
        reply.error(ENOENT);
    }


    fn lookup_photo(&mut self, name: &OsStr, reply: ReplyEntry) {
        if let Some(FileId::FileKind(id)) = self.extract_id(name) {
            let mut statement = self.conn.prepare("SELECT filesize, timestamp FROM PhotoTable WHERE id = ?").unwrap();
            statement.bind(1, id as i64).unwrap();
            if let Ok(sqlite::State::Row) = statement.next() {
                let timestamp = time::Timespec{sec: statement.read::<i64>(1).unwrap(), nsec: 0};
                let filesize = statement.read::<i64>(0).unwrap() as u64;
                reply.entry(&TTL, &make_fileattr(PHOTO | id, filesize, timestamp), 0);
                return;
            }
        }
        reply.error(ENOENT);
    }

    fn lookup_video(&mut self, name: &OsStr, reply: ReplyEntry) {
        if let Some(FileId::FileKind(id)) = self.extract_id(name) {
            let mut statement = self.conn.prepare("SELECT filesize, timestamp FROM VideoTable WHERE id = ?").unwrap();
            statement.bind(1, id as i64).unwrap();
            if let Ok(sqlite::State::Row) = statement.next() {
                let timestamp = time::Timespec{sec: statement.read::<i64>(1).unwrap(), nsec: 0};
                let filesize = statement.read::<i64>(0).unwrap() as u64;
                reply.entry(&TTL, &make_fileattr(VIDEO | id, filesize, timestamp), 0);
                return;
            }
        }
        reply.error(ENOENT);

    }
}

impl Filesystem for ShotwellVFS {
    fn lookup(&mut self,
              _: &fuse::Request,
              parent: u64,
              name: &OsStr,
              reply: ReplyEntry,
              ) {
        match parent {
            ROOT => self.lookup_root(name, reply),
            EVENT => self.lookup_event(name, reply),
            PHOTO => self.lookup_photo(name, reply),
            VIDEO => self.lookup_video(name, reply),
            TAG => self.lookup_tag(name, reply),
            _ => reply.error(ENOENT),
        };
    }

    fn getattr(&mut self,
               _: &fuse::Request,
               inode: u64,
               reply: ReplyAttr,
               ) {
        match inode {
            ROOT => reply.attr(&TTL, &ROOT_ATTR),
            PHOTO => reply.attr(&TTL, &PHOTO_ATTR),
            VIDEO => reply.attr(&TTL, &VIDEO_ATTR),
            TAG => reply.attr(&TTL, &TAG_ATTR),
            EVENT => reply.attr(&TTL, &EVENT_ATTR),
            _ => reply.error(ENOENT),
        };
    }

    fn readdir(&mut self,
               _: &fuse::Request,
               inode: u64,
               fh: u64,
               offset: i64,
               reply: fuse::ReplyDirectory,
               ) {
        match inode {
            ROOT => self.readdir_root(reply, offset),
            PHOTO => self.readdir_photos(reply, offset),
            VIDEO => self.readdir_videos(reply, offset),
            TAG => self.readdir_tags(reply, offset),
            EVENT => self.readdir_events(reply, offset),
            _ => reply.error(ENOENT)
        };
    }
}

fn main() {
    env_logger::init().unwrap();
    let mountpoint = std::env::args_os().nth(1).unwrap();
    fuse::mount(ShotwellVFS::new("/home/torkve/.local/share/shotwell/data/photo.db"), &mountpoint, &[]).unwrap();
}
