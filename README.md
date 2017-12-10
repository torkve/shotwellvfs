Shotwell VFS
============

[![license](http://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/torkve/shotwellvfs/blob/master/LICENSE)
[![Build Status](https://travis-ci.org/torkve/shotwellvfs.svg?branch=master)](https://travis-ci.org/torkve/shotwellvfs)

Expose your [Shotwell](https://wiki.gnome.org/Apps/Shotwell) library as a filesystem mountpoint.

Warning: currently is extremely unstable, lacks most of the functionality. Use it on your own risk and don't use
simultaneously with Shotwell: the result will be inconsistent.

Build
-----

You will need [libfuse](https://github.com/libfuse/libfuse) to build this library. On Ubuntu- and Debian-based systems it
can be installed with `$ sudo apt install libfuse-dev`.

To build shotwellvfs use cargo package manager: `$ cargo build --release`

Usage
-----

To start currently just use: `$ target/release/shotwellvfs MOUNTPOINT` where `MOUNTPOINT` is a directory where the library should be mounted.

To unmount it use `$ fusermount -u MOUNTPOINT`
