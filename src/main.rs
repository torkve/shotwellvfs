extern crate fuse;
#[macro_use] extern crate log;
extern crate env_logger;
#[macro_use] extern crate clap;

mod sqlite_ex;
mod fuse_interface;

use fuse_interface::ShotwellVFS;

fn main() {
    env_logger::init().unwrap();
    let args = clap::App::new("Shotwell VFS")
        .version(crate_version!())
        .author("Vsevolod Velichko <torkvemada@sorokdva.net")
        .about("Expose shotwell library as filesystem hierarchy")
        .arg(clap::Arg::with_name("db")
             .long("db")
             .value_name("FILE")
             .help("Custom path to database file")
             .takes_value(true)
            )
        .arg(clap::Arg::with_name("MOUNTPOINT")
             .help("Path to mount FS")
             .required(true)
             .index(1)
             )
        .get_matches();

    let mountpoint = args.value_of("MOUNTPOINT").unwrap();
    let vfs = match args.value_of("db") {
        None => {
            let mut path = std::env::home_dir().unwrap_or_else(|| panic!("Cannot find user home dir and no --db argument specified"));
            path.push(".local/share/shotwell/data/photo.db");
            ShotwellVFS::new(path)
        },
        Some(path) => ShotwellVFS::new(path),
    };
    fuse::mount(vfs, &mountpoint, &[]).unwrap();
}
