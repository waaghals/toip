use indicatif::ProgressStyle;

pub fn bytes_style() -> ProgressStyle {
    ProgressStyle::default_bar()
        .template("{msg} {bar:40.cyan/blue} {bytes}/{total_bytes} ({bytes_per_sec})")
        .progress_chars("##-")
}
