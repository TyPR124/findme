#![forbid(unsafe_code)]

use std::fmt::{Display, Formatter, Result as FmtResult};
use std::fs;
use std::fs::{DirEntry, Metadata};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use std::num::NonZeroUsize;
type Result = std::result::Result<(), String>;

const NEWEST_MODIFIED: &str = "newest-modified";
const NEWEST_CREATED: &str = "newest-created";
const NEWEST_ACCESSED: &str = "newest-accessed";
const OLDEST_MODIFIED: &str = "oldest-modified";
const OLDEST_CREATED: &str = "oldest-created";
const OLDEST_ACCESSED: &str = "oldest-accessed";
const LARGEST: &str = "largest";
const SMALLEST: &str = "smallest";

trait OutputHandler {
    fn handle(&mut self, text: &str);
}
struct Subject {
    files: bool,
    dirs: bool
}
struct Flags {
    recursive: bool,
    follow_symlinks: bool
}
fn match_op(s: &str) -> Option<&'static str> {
    // For user safety, do not match on fewer than 2 chars
    if s.len() < 2 { return None }
    const VALUES: [&str; 8] = [
        NEWEST_MODIFIED,
        NEWEST_CREATED,
        NEWEST_ACCESSED,
        LARGEST,
        OLDEST_MODIFIED,
        OLDEST_CREATED,
        OLDEST_ACCESSED,
        SMALLEST
    ];
    let lower = s.to_ascii_lowercase();
    let mut ret = None;
    for &value in &VALUES {
        if value.starts_with(&lower) {
            if ret.is_some() { return None }
            ret = Some(value);
        }
    }
    ret
}

fn main() {
    use clap::{App, load_yaml, ErrorKind::InvalidValue};
    let args_yaml = load_yaml!("command-line.yml"); // At compile-time
    let args = App::from_yaml(args_yaml).get_matches(); // At run-time
    fn invalid_arg_exit(msg: &str) -> ! {
        clap::Error::with_description(msg, InvalidValue).exit()
    }

    let subject = Subject {
        files: args.is_present("files"),
        dirs: args.is_present("dirs")
    };
    let flags = Flags {
        recursive: args.is_present("recursive"),
        follow_symlinks: args.is_present("symlinks")
    };
    let count = NonZeroUsize::new(args.value_of("count").unwrap() // Arg required
        .parse().unwrap_or(0)) // 0-case fails to invalid_arg_exit
    .unwrap_or_else(||{
        invalid_arg_exit("count must be a non-zero positive integer");
    });
    let op_raw = args.value_of("op").unwrap(); // Arg required
    let op = match_op(op_raw) // Arg required
    .unwrap_or_else(|| {
        invalid_arg_exit(&format!("'{}' is not a valid op. See --help for more info.", op_raw));
    });
    let max_subdirs = args.value_of("max-subdirs").unwrap() // Arg required
    .parse().unwrap_or_else(|_| {
        invalid_arg_exit("max-subdirs must be a positive integer or 0");
    });
    let paths = args.values_of("paths").unwrap() // Arg required
        .map(|s|s.into()).collect();
    let mut out: Box<dyn OutputHandler> = Box::new(Print);
    let mut out_err: Box<dyn OutputHandler> = if args.is_present("ignore-errors") {
        Box::new(Ignore)
    } else { Box::new(Print) };

    macro_rules! do_it {
        ($OP:ident) => {
            findme::<$OP>(count, subject, flags, max_subdirs,
                          paths, out.as_mut(), out_err.as_mut());
        };
    }
    match op {
        NEWEST_MODIFIED => do_it!(NewestModified),
        NEWEST_CREATED =>  do_it!(NewestCreated),
        NEWEST_ACCESSED => do_it!(NewestAccessed),
        OLDEST_MODIFIED => do_it!(OldestModified),
        OLDEST_CREATED =>  do_it!(OldestCreated),
        OLDEST_ACCESSED => do_it!(OldestAccessed),
        LARGEST =>         do_it!(Largest),
        SMALLEST =>        do_it!(Smallest),
        _ => unreachable!("Invalid op from match_op")
    }
} // End main

trait Picker {
    type Output: Display;
    fn new(count: NonZeroUsize) -> Self;
    fn choice(&mut self, item: &DirEntry, meta: &Metadata) -> Result;
    fn finish(self) -> Vec<Self::Output>;
}

// Finds files according to Picker, and outputs the results
fn findme<P: Picker>(
          count: NonZeroUsize,
        subject: Subject,
          flags: Flags,
    max_subdirs: u16,
     init_paths: Vec<PathBuf>,
            out: &mut dyn OutputHandler,
         errout: &mut dyn OutputHandler,
) {
    let mut files_checked: usize = 0;
    let mut dirs_checked: usize = 0;
    let mut picker = P::new(count);
    let mut paths = Vec::with_capacity(init_paths.len());
    for path in init_paths {
        paths.push((path, 0)); // dir_level = 0
    }
    let fs_metadata: &Fn(&Path) -> std::io::Result<Metadata> = 
        if flags.follow_symlinks { &|path| fs::metadata(path) }
        else { &|path| fs::symlink_metadata(path) };

    let start_time = SystemTime::now();

    while let Some((path, dir_level)) = paths.pop() {
        let root = match fs::read_dir(&path) {
            Ok(dir_iter) => dir_iter,
            Err(_) => {
                errout.handle(&format!("Unable to read directory: {:?}", path));
                continue
            }
        };
        for maybe_entry in root {
            let entry = match maybe_entry {
                Ok(entry) => entry,
                Err(_) => {
                    errout.handle(&format!("Unable to read entry in {:?}", path));
                    continue
                }
            };
            let entry_path = entry.path();
            let meta = match fs_metadata(&entry_path) {
                Ok(meta) => meta,
                Err(_) => {
                    errout.handle(&format!("Unable to read metadata: {:?}", entry_path));
                    continue
                }
            };

            if meta.is_file() {
                files_checked += 1;
                if subject.files {
                    picker.choice(&entry, &meta).unwrap_or_else(|e| errout.handle(&e));
                }
            } else if meta.is_dir() {
                dirs_checked += 1;
                if subject.dirs {
                    picker.choice(&entry, &meta).unwrap_or_else(|e| errout.handle(&e));
                }
                if flags.recursive && dir_level < max_subdirs {
                    paths.push((entry_path, dir_level + 1));
                }
            }
        } // End for entry in root
    } // End while iterating paths
    let end_time = SystemTime::now();
    let time = end_time.duration_since(start_time).expect("Time error!");
    println!("This took {}.{:03} seconds", time.as_secs(), time.subsec_millis());
    let results = picker.finish();
    let subj_text = if subject.files & subject.dirs { "files/dirs" }
        else if subject.files { "files" }
        else { "dirs" };
    out.handle(&format!("Top {} {} out of {} files in {} dirs:",
                        count, subj_text, files_checked, dirs_checked));
    for res in &results {
        out.handle(&format!("{}", res));
    }
} // End findme

macro_rules! make_pickers {
    ( $(
        $Name:ident => finds($meta_item:ident $gt_lt:tt $Entry:ident . $entry_value:ident)
                       $(unless ($err:expr))?  
    ),+ $(,)? ) => { $(

        struct $Name {
            vec: Vec<$Entry>
        }
        impl Picker for $Name {
            type Output = $Entry;
            fn new(count: NonZeroUsize) -> Self {
                Self { vec: Vec::with_capacity(count.get()) }
            }
            fn choice(&mut self, item: &DirEntry, meta: &Metadata) -> Result {
                let value = meta.$meta_item() $(
                    .map_err(|_| format!("{}: {:?}", $err, item.path()))?
                )? ;
                // Capacity must be non-zero, so just insert if len == 0
                if self.vec.len() == 0 {
                    self.vec.push($Entry::new(item.path(), value));
                    return Ok(())
                }
                // If value is not better than worst item...
                else if !(value $gt_lt self.vec[self.vec.len()-1].$entry_value) {
                    // ...then push if there is space
                    if self.vec.len() < self.vec.capacity() {
                        self.vec.push($Entry::new(item.path(), value));
                    }
                    return Ok(())
                }
                // Value is better than worst item, so this loop must return
                for i in 0..self.vec.len() {
                    if value $gt_lt self.vec[i].$entry_value {
                        if self.vec.len() == self.vec.capacity() {
                            self.vec.pop()
                            .expect("pop failed from a non-zero-len vec");
                        }
                        self.vec.insert(i, $Entry::new(item.path(), value));
                        return Ok(())
                    }
                }
                unreachable!("picker.choice failed to find insertion point")
            }
            fn finish(self) -> Vec<Self::Output> { self.vec }
        }
    )+ };
} // End make_pickers definition

make_pickers! {
    NewestModified => finds(modified > TimeEntry.time)
                      unless("Could not read modified time"),
    NewestCreated  => finds(created > TimeEntry.time)
                      unless("Could not read created time"),
    NewestAccessed => finds(accessed > TimeEntry.time)
                      unless("Could not read accessed time"),
    OldestModified => finds(modified < TimeEntry.time)
                      unless("Could not read modified time"),
    OldestCreated  => finds(created < TimeEntry.time)
                      unless("Could not read created time"),
    OldestAccessed => finds(accessed < TimeEntry.time)
                      unless("Could not read accessed time"),
    Largest        => finds(len > LenEntry.len),
    Smallest       => finds(len < LenEntry.len),
}
struct TimeEntry {
    path: PathBuf,
    time: SystemTime
}
struct LenEntry {
    path: PathBuf,
    len: u64
}
impl TimeEntry {
    fn new(path: PathBuf, time: SystemTime) -> Self {
        Self { path, time }
    }
}
impl LenEntry {
    fn new(path: PathBuf, len: u64) -> Self {
        Self { path, len }
    }
}
impl Display for TimeEntry {
    fn fmt(&self, out: &mut Formatter) -> FmtResult {
        use chrono::{DateTime, Local};
        let dt: DateTime<Local> = DateTime::from(self.time);
        write!(out, "{} : {}", dt, self.path.display())
    }
}
impl Display for LenEntry {
    fn fmt(&self, out: &mut Formatter) -> FmtResult {
        write!(out, "{} : {}", self.len, self.path.display())
    }
}

struct Ignore;
impl OutputHandler for Ignore {
    fn handle(&mut self, _: &str) {}
}
struct Print;
impl OutputHandler for Print {
    fn handle(&mut self, s: &str) { println!("{}", s) }
}
impl OutputHandler for std::io::Write {
    fn handle(&mut self, s: &str) {
        writeln!(self, "{}", s).unwrap_or_else(|e| {
            println!("Output handler encountered error: {}", e)
        })
    }
}