# Project plan

A small command line tool to **sort photos** by the date they were taken,
read from their EXIF data.[^exif]

## Milestones

| Milestone | Owner | Due | Status |
| :-- | :-- | --: | :-: |
| Read EXIF dates | Ana | Oct 12 | done |
| Move files into folders | Tomás | Oct 19 | in progress |
| Dry-run mode | Ana | Oct 26 | planned |

## Tasks

- [x] Pick an EXIF library
- [x] Write the folder naming rule
- [ ] Handle photos ~~without~~ with a missing date
- [ ] Package for Debian

## Folder naming

```rust
fn folder(date: NaiveDate) -> PathBuf {
    // 2026/10 - October
    let name = format!("{:02} - {}", date.month(), date.format("%B"));
    PathBuf::from(date.year().to_string()).join(name)
}
```

> Keep the originals until the dry run has been checked.

[^exif]: Exchangeable image file format, written by most cameras.
