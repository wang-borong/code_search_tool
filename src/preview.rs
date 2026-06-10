use crate::errors::Result;
use crate::search::SearchResult;
use bat::line_range::{LineRange, LineRanges};

pub fn preview_to_string(result: &SearchResult, height: usize) -> Result<String> {
    preview_path(&result.path, result.line_num, height)
}

pub fn preview_path(path: &str, line_num: usize, height: usize) -> Result<String> {
    let start = if line_num > height * 3 / 4 {
        line_num - height * 3 / 4
    } else {
        1
    };
    let end = start + height * 3 - 1;

    let range = LineRange::new(start, end);
    let ranges = LineRanges::from(vec![range]);

    let config = crate::config::get_global();
    let tab_width = config.skim.tab_width;

    let mut output_str = String::new();
    let mut printer = bat::PrettyPrinter::new();
    printer
        .input_file(path)
        .line_ranges(ranges)
        .highlight(line_num)
        .grid(true)
        .line_numbers(true)
        .colored_output(true)
        .true_color(true)
        .term_width(100)
        .tab_width(Some(tab_width))
        .print_with_writer(Some(&mut output_str))
        .map_err(|e| crate::errors::AppError::General(e.to_string()))?;

    Ok(output_str)
}

pub fn preview(result: &SearchResult, height: usize) -> Result<()> {
    let output = preview_to_string(result, height)?;
    print!("{output}");
    Ok(())
}
