//! Example of a stateless source reading from files on the local filesystem
use core::iter::Enumerate;
use std::{
    fs::File,
    io::{BufRead as _, BufReader, Lines},
    iter::Peekable,
};
use {
    malstrom::operators::Source as _,
    malstrom::runtime::SingleThreadRuntime,
    malstrom::snapshot::NoPersistence,
    malstrom::sources::{Source, SourceImpl, SourcePartition},
    malstrom::worker::StreamProvider,
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

/// Implement the source emitting String values and usize timestamps.
/// A stateless source is just a `SourceImpl` with `PartitionState = ()`.
impl SourceImpl for FileSource {
    // we will create one partition per path (String)
    type PartitionKey = String;
    type Value = String;
    type Timestamp = usize;
    type Partition = FileSourcePartition;
    type PartitionState = ();

    async fn discover(&mut self) -> Vec<Self::PartitionKey> {
        self.paths.clone()
    }

    async fn open(
        &mut self,
        part: &Self::PartitionKey,
        _state: Option<Self::PartitionState>,
    ) -> Self::Partition {
        FileSourcePartition::new(part.clone())
    }
}
// #endregion source_impl
// #region partition_impl
type FileLines = Peekable<Enumerate<Lines<BufReader<File>>>>;
/// Reads lines from a single file
struct FileSourcePartition {
    path: String,
    file: Option<FileLines>,
}
impl FileSourcePartition {
    fn new(path: String) -> Self {
        Self { path, file: None }
    }
}

impl SourcePartition for FileSourcePartition {
    type PartitionKey = String;
    type Value = String;
    type Timestamp = usize;
    type State = ();

    async fn poll(&mut self) -> Option<(Self::Value, Self::Timestamp)> {
        // open the file
        let file = self.file.get_or_insert_with(|| {
            BufReader::new(File::open(&self.path).unwrap())
                .lines()
                .enumerate()
                .peekable()
        });
        file.next().map(|(i, x)| (x.unwrap(), i))
    }

    async fn snapshot(&self) -> Self::State {}

    async fn collect(self) -> Self::State {}
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
