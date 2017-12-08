extern crate libc;
extern crate time;
extern crate fuse;
extern crate sqlite;
#[macro_use] extern crate log;
extern crate env_logger;

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
struct ShotwellVFS;

impl ShotwellVFS {
    fn extract_id(&self, filename: &OsStr) -> Option<u64> {
        filename
            .to_str()
            .into_iter()
            .filter(|x| x.starts_with('['))
            .next()
            .and_then(|x| x.find(']').and_then(|end| x[1..end].parse::<u64>().ok()))
    }

    fn fill_events(&mut self, mut reply: fuse::ReplyDirectory, offset: i64) {
        if offset < 0 {
            reply.error(ENOENT);
            return;
        }
        if offset == 0 {
            reply.add(EVENT, 0, FileType::Directory, ".");
            reply.add(ROOT, 1, FileType::Directory, "..");
        }
        let mut idx = offset + 2;

        let connection = sqlite::open("/home/torkve/.local/share/shotwell/data/photo.db").unwrap();
        let mut statement = connection.prepare("SELECT id, name, time_created FROM EventTable ORDER BY time_created ASC LIMIT ?, 100").unwrap();
        statement.bind(1, offset).unwrap();
        while let Ok(sqlite::State::Row) = statement.next() {
            let event_id = statement.read::<i64>(0).unwrap() as u64;
            let inode = event_id | EVENT;
            let name = statement.read::<Vec<u8>>(1).map_err(|_|()).and_then(|x| String::from_utf8(x).map_err(|_|())).unwrap_or(String::new());
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
}

impl Filesystem for ShotwellVFS {
    fn lookup(&mut self,
              _: &fuse::Request,
              parent: u64,
              name: &OsStr,
              reply: ReplyEntry,
              ) {
        match parent {
            ROOT => match name.to_str() {
                Some("/") => reply.entry(&TTL, &ROOT_ATTR, 0),
                Some("photos") => reply.entry(&TTL, &PHOTO_ATTR, 0),
                Some("videos") => reply.entry(&TTL, &VIDEO_ATTR, 0),
                Some("tags") => reply.entry(&TTL, &TAG_ATTR, 0),
                Some("events") => reply.entry(&TTL, &EVENT_ATTR, 0),
                _ => reply.error(ENOENT),
            },
            EVENT => {
                if let Some(id) = self.extract_id(name) {
                    let connection = sqlite::open("/home/torkve/.local/share/shotwell/data/photo.db").unwrap();
                    let mut statement = connection.prepare("SELECT time_created FROM EventTable WHERE id = ?").unwrap();
                    statement.bind(1, id as i64).unwrap();
                    if let Ok(sqlite::State::Row) = statement.next() {
                        let timestamp = time::Timespec{sec: statement.read::<i64>(0).unwrap(), nsec: 0};
                        reply.entry(&TTL, &make_dirattr(EVENT | id, timestamp), 0);
                    } else {
                        reply.error(ENOENT);
                    }
                } else {
                    reply.error(ENOENT);
                }
            }
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
               mut reply: fuse::ReplyDirectory,
               ) {
        if inode == EVENT {
            self.fill_events(reply, offset);
        }
        else if offset != 0 {
            reply.error(ENOENT);
        } else {
            match inode {
                ROOT => {
                    reply.add(ROOT, 0, FileType::Directory, ".");
                    reply.add(ROOT, 1, FileType::Directory, "..");
                    reply.add(PHOTO, 2, FileType::Directory, "photos");
                    reply.add(VIDEO, 3, FileType::Directory, "videos");
                    reply.add(TAG, 4, FileType::Directory, "tags");
                    reply.add(EVENT, 5, FileType::Directory, "events");
                    reply.ok()
                },
                PHOTO => {
                    reply.add(PHOTO, 0, FileType::Directory, ".");
                    reply.add(ROOT, 1, FileType::Directory, "..");
                    reply.ok()
                },
                VIDEO => {
                    reply.add(VIDEO, 0, FileType::Directory, ".");
                    reply.add(ROOT, 1, FileType::Directory, "..");
                    reply.ok()
                },
                TAG => {
                    reply.add(TAG, 0, FileType::Directory, ".");
                    reply.add(ROOT, 1, FileType::Directory, "..");
                    reply.ok()
                },
                EVENT => self.fill_events(reply, offset),
                _ => reply.error(ENOENT)
            }
        };
    }
}

fn main() {
    env_logger::init().unwrap();
    let mountpoint = std::env::args_os().nth(1).unwrap();
    fuse::mount(ShotwellVFS, &mountpoint, &[]).unwrap();
}
