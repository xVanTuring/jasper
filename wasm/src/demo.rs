//! 内置演示库（纯文本，无资源/图片），编进 WASM，供浏览器零服务器试用。
//! 字段精简但满足 parser；与 docs/gen-demo-library.py 的内容风格一致。

fn folder(id: &str, title: &str, parent: &str) -> String {
    format!(
        "{title}\n\nid: {id}\nparent_id: {parent}\n\
         created_time: 2026-06-10T09:00:00.000Z\nupdated_time: 2026-06-10T09:00:00.000Z\ntype_: 2"
    )
}

fn note(id: &str, parent: &str, title: &str, body: &str, t: &str) -> String {
    format!(
        "{title}\n\n{body}\n\nid: {id}\nparent_id: {parent}\n\
         created_time: {t}\nupdated_time: {t}\nuser_created_time: {t}\nuser_updated_time: {t}\n\
         markup_language: 1\nis_todo: 0\ntodo_completed: 0\nsource_url: \ntype_: 1"
    )
}

const SHOWCASE: &str = r#"这是一篇在**浏览器里**由 Rust → WASM 解析渲染的笔记——后端逻辑没有 server，全跑在你这个标签页里。

## 它能做什么
- 直接读写本地文件夹或 **WebDAV** 上的 Joplin 同步库（完整版）
- Markdown 实时渲染：代码高亮、表格、数学公式、任务清单
- 资源/图片：粘贴、拖拽、附件上传，并可在面板里管理

> 这个 demo 用的是内置演示库，解析器 / 索引和真实后端是**同一份 Rust 代码**（jasper-core）。

### 代码高亮
```rust
#[wasm_bindgen]
pub fn folders(&self) -> String {
    serde_json::to_string(&self.tree()).unwrap()
}
```

### 表格
| 运行位置 | 解析 | 索引 | 搜索 |
|----------|------|------|------|
| 原生 server | ✅ | ✅ | ✅ |
| 浏览器 WASM | ✅ | ✅ | ✅ |

### 任务清单
- [x] 抽出 jasper-core 纯逻辑 crate
- [x] 编译到 wasm32
- [x] 浏览器里出笔记本树 + 渲染笔记
- [ ] WASM 里也支持写入 / 资源

### 数学公式
行内 $E = mc^2$，独立公式：

$$\int_0^\infty e^{-x}\,dx = 1$$
"#;

const RUST_NOTE: &str = r#"所有权三条规则：

1. 每个值有且仅有一个**所有者**
2. 同一时刻只能有一个所有者
3. 所有者离开作用域，值被丢弃

```rust
fn longest<'a>(x: &'a str, y: &'a str) -> &'a str {
    if x.len() > y.len() { x } else { y }
}
```

借用检查器在**编译期**保证内存安全，无需 GC。
"#;

const PLAN_NOTE: &str = r#"## 目标
- 抽 core、编 wasm、跑通浏览器 demo

| 里程碑 | 状态 |
|--------|------|
| core crate | ✅ |
| wasm demo | ✅ |
| 写入支持 | 🚧 |
"#;

const READING_NOTE: &str = r#"- [x] 《Rust 程序设计语言》
- [x] 《深入理解计算机系统》
- [ ] 《数据密集型应用系统设计》

> 读完做一页纸笔记。
"#;

/// 返回演示库的全部条目（`.md` 原始文本）。
pub fn items() -> Vec<String> {
    let tech = "a1110000000000000000000000000001";
    let work = "a1110000000000000000000000000002";
    let pers = "a1110000000000000000000000000003";
    vec![
        folder(tech, "技术笔记 · Tech", ""),
        folder(work, "工作 · Work", ""),
        folder(pers, "个人 · Personal", ""),
        note(
            "c1110000000000000000000000000001",
            tech,
            "✨ 功能展示 Feature Showcase",
            SHOWCASE,
            "2026-06-28T15:20:00.000Z",
        ),
        note(
            "c1110000000000000000000000000002",
            tech,
            "Rust 所有权与借用",
            RUST_NOTE,
            "2026-06-26T11:00:00.000Z",
        ),
        note(
            "c1110000000000000000000000000003",
            work,
            "Q3 项目计划",
            PLAN_NOTE,
            "2026-06-29T09:30:00.000Z",
        ),
        note(
            "c1110000000000000000000000000004",
            pers,
            "📚 读书清单",
            READING_NOTE,
            "2026-06-22T20:00:00.000Z",
        ),
    ]
}
