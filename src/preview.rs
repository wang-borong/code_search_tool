use crate::errors::Result;
use crate::search::SearchResult;
use bat::line_range::{LineRange, LineRanges};

pub fn preview_to_string(result: &SearchResult, height: usize) -> Result<String> {
    let start = if result.line_num > height * 3 / 4 {
        result.line_num - height * 3 / 4
    } else {
        1
    };
    let end = start + height * 3 - 1;

    let range = LineRange::new(start, end);
    let ranges = LineRanges::from(vec![range]);

    let mut output_str = String::new();
    let mut printer = bat::PrettyPrinter::new();
    printer
        .input_file(&result.path)
        .line_ranges(ranges)
        .highlight(result.line_num)
        .grid(true)
        .line_numbers(true)
        .colored_output(true)
        .true_color(true)
        .term_width(100)
        .print_with_writer(Some(&mut output_str))
        .map_err(|e| crate::errors::AppError::General(e.to_string()))?;

    Ok(output_str)
}

pub fn preview(result: &SearchResult, height: usize) -> Result<()> {
    let output = preview_to_string(result, height)?;
    print!("{output}");
    Ok(())
}
