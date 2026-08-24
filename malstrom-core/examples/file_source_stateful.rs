//! Example of a stateful source reading from files on the local filesystem
use core::iter::Enumerate;
use malstrom::{
    operators::Source as _,
    runtime::SingleThreadRuntime,
    snapshot::NoPersistence,
    sources::{Source, SourceImpl, SourcePartition},
    worker::StreamProvider,
};
use std::{
    fs::File,
    io::{BufRead as _, BufReader, Lines},
    iter::{Peekable, Skip},
};
// #region source_impl
/// Reads lines from files and emits them as records
struct FileSource {
    paths: Vec<String>, // file paths
}

impl FileSource {
    pub fn new(paths: Vec<String>) -> Self {
        Self { paths }
    }
}

/// Implement the source emitting String values and usize timestamps
impl SourceImpl for FileSource {
    // we will create one partition per path (String)
    type PartitionKey = String;
    type Value = String;
    type Timestamp = usize;
    type Partition = FileSourcePartition;
    type PartitionState = usize;

    async fn discover(&mut self) -> Vec<Self::PartitionKey> {
        self.paths.clone()
    }

    async fn open(
        &mut self,
        part: &Self::PartitionKey,
        state: Option<Self::PartitionState>,
    ) -> Self::Partition {
        FileSourcePartition::new(part.clone(), state.unwrap_or(0))
    }
}
// #endregion source_impl
// #region partition_impl
type FileLines = Peekable<Skip<Enumerate<Lines<BufReader<File>>>>>;
/// Reads lines from a single file
struct FileSourcePartition {
    path: String,
    file: Option<FileLines>,
    next_line: usize,
}
impl FileSourcePartition {
    fn new(path: String, next_line: usize) -> Self {
        Self {
            path,
            file: None,
            next_line,
        }
    }
}

impl SourcePartition for FileSourcePartition {
    type PartitionKey = String;
    type State = usize;
    type Value = String;
    type Timestamp = usize;

    async fn poll(&mut self) -> Option<(Self::Value, Self::Timestamp)> {
        // open the file
        let file = self.file.get_or_insert_with(|| {
            BufReader::new(File::open(&self.path).unwrap())
                .lines()
                .enumerate()
                .skip(self.next_line)
                .peekable()
        });
        file.next().map(|(i, x)| {
            self.next_line += 1;
            (x.unwrap(), i)
        })
    }

    async fn snapshot(&self) -> Self::State {
        self.next_line
    }

    async fn collect(self) -> Self::State {
        self.next_line
    }
}
// #endregion partition_impl
// #region usage
fn build_dataflow(provider: &mut dyn StreamProvider) {
    provider.new_stream().source(
        "files",
        Source::from_impl(FileSource::new(vec![
            "/some/path.txt".to_string(),
            "/some/other/path.txt".to_string(),
        ])),
    );
}
// #endregion usage
fn main() {
    let _rt = SingleThreadRuntime::builder()
        .persistence(NoPersistence)
        .build(build_dataflow)
        .execute();
}
