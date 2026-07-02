//! 去行尾空白：`hook.before_save` 的最小参考实现（spec 附录 B）。

use jasper_plugin_sdk as sdk;
use sdk::core::model::Note;

fn before_save(mut note: Note) -> Result<Note, String> {
    note.body = note.body.lines().map(|l| l.trim_end()).collect::<Vec<_>>().join("\n");
    Ok(note)
}

sdk::register! { before_save: before_save }
