use lume_core::buffer::visual;

pub(crate) fn display_width(text: &str) -> usize {
    visual::graphemes(text).map(|g| g.width).sum()
}

pub(crate) fn fit_text(text: &str, width: usize) -> String {
    let mut result = String::new();
    let mut used = 0;
    for grapheme in visual::graphemes(text) {
        if grapheme.column + grapheme.width > width {
            break;
        }
        if grapheme.width > 0 {
            result.push_str(grapheme.display_text());
            used += grapheme.width;
        }
    }
    result.extend(std::iter::repeat_n(' ', width - used));
    result
}
