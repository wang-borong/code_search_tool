use skim::prelude::*;
use std::sync::Arc;

use fcs::core::CodeItem;
use fcs::errors::AppError;

pub(super) fn run_code_item_picker(
    items: &[CodeItem],
    query: Option<&String>,
    config: &fcs::config::Config,
) -> Result<(), AppError> {
    let mut current_query = query.cloned().unwrap_or_default();
    loop {
        let (sender, receiver): (SkimItemSender, SkimItemReceiver) = unbounded();
        let skim_items = items
            .iter()
            .map(|item| Arc::new(item.clone()) as Arc<dyn SkimItem>)
            .collect::<Vec<Arc<dyn SkimItem>>>();
        let _ = sender.send(skim_items);
        drop(sender);

        let skim_options = SkimOptionsBuilder::default()
            .height(config.skim.height.as_str())
            .min_height(config.skim.min_height.as_str())
            .multi(true)
            .color(config.skim.color.as_str())
            .exact(config.skim.exact)
            .tac(config.skim.tac)
            .cycle(config.skim.cycle)
            .bind(config.skim.binds.clone())
            .preview("")
            .preview_window(config.skim.preview_window.as_str())
            .query(current_query.clone())
            .build()
            .map_err(|err| AppError::Skim(err.to_string()))?;

        let output = match Skim::run_with(skim_options, Some(receiver)) {
            Ok(output) => output,
            Err(_) => break,
        };
        current_query = output.query.clone();
        if output.is_abort {
            break;
        }

        for selected_item in output.selected_items.iter() {
            let display = selected_item.output().to_string();
            if let Some(item) = items.iter().find(|item| item.display_text() == display) {
                fcs::trace::record_code_item(item, "open")?;
                fcs::editor::open_location(&item.location, config.editor.command.as_deref())?;
            }
        }
    }

    Ok(())
}
