pub fn display_join<I, T>(it: I, sep: &str) -> String
where
    I: IntoIterator<Item = T>,
    T: std::fmt::Display,
{
    use std::fmt::Write;

    let mut it = it.into_iter();
    let first = it.next().map(|f| f.to_string()).unwrap_or_default();

    it.fold(first, |mut acc, s| {
        write!(acc, "{}{}", sep, s).expect("Writing in a String shouldn't fail");
        acc
    })
}
