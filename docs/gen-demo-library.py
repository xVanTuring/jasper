#!/usr/bin/env python3
# 生成一个演示用 Joplin 同步库（笔记本 + 富文本笔记 + 一张 SVG 资源），用于试用 / 截图。
# 用法：
#   python3 docs/gen-demo-library.py /tmp/jasper-demo
#   cd server && cargo run -- /tmp/jasper-demo
import os, sys

if len(sys.argv) < 2:
    print("用法: python3 gen-demo-library.py <输出目录>")
    sys.exit(1)
ROOT = sys.argv[1]
RES = os.path.join(ROOT, ".resource")
os.makedirs(RES, exist_ok=True)

def iso(s): return s  # 直接给 ISO 串

def folder(fid, title, parent, t):
    props = [
        f"id: {fid}", f"created_time: {t}", f"updated_time: {t}",
        f"user_created_time: {t}", f"user_updated_time: {t}",
        "encryption_cipher_text: ", "encryption_applied: 0",
        f"parent_id: {parent}", "is_shared: 0", "share_id: ",
        "master_key_id: ", "icon: ", "user_data: ", "deleted_time: 0", "type_: 2",
    ]
    return f"{title}\n\n" + "\n".join(props)

def note(nid, parent, title, body, t):
    props = [
        f"id: {nid}", f"parent_id: {parent}", f"created_time: {t}", f"updated_time: {t}",
        "is_conflict: 0", "latitude: 0.00000000", "longitude: 0.00000000", "altitude: 0.0000",
        "author: ", "source_url: ", "is_todo: 0", "todo_due: 0", "todo_completed: 0",
        "source: jasper-web", "source_application: net.cozic.jasper-web", "application_data: ",
        "order: 0", f"user_created_time: {t}", f"user_updated_time: {t}",
        "encryption_cipher_text: ", "encryption_applied: 0", "markup_language: 1",
        "is_shared: 0", "share_id: ", "conflict_original_id: ", "master_key_id: ",
        "user_data: ", "deleted_time: 0", "type_: 1",
    ]
    return f"{title}\n\n{body}\n\n" + "\n".join(props)

def resource(rid, title, mime, ext, size, t):
    props = [
        f"id: {rid}", f"mime: {mime}", "filename: ",
        f"created_time: {t}", f"updated_time: {t}", f"user_created_time: {t}", f"user_updated_time: {t}",
        f"file_extension: {ext}", "encryption_cipher_text: ", "encryption_applied: 0",
        "encryption_blob_encrypted: 0", f"size: {size}", "is_shared: 0", "share_id: ",
        "master_key_id: ", "user_data: ", "blob_updated_time: 1718000000000",
        "ocr_text: ", "ocr_details: ", "ocr_status: 0", "ocr_error: ", "ocr_driver_id: 1", "type_: 4",
    ]
    return f"{title}\n\n" + "\n".join(props)

def write(name, content):
    with open(os.path.join(ROOT, name), "w", encoding="utf-8") as f:
        f.write(content)

# info.json（v3，未加密）
write("info.json", '{"version":3,"e2ee":{"value":false,"updatedTime":0},"activeMasterKeyId":{"value":"","updatedTime":0},"masterKeys":[],"ppk":{"value":null,"updatedTime":0},"appMinVersion":"3.0.0"}')

# ---- 固定 id（32 hex）----
F_TECH = "a1110000000000000000000000000001"
F_WORK = "a1110000000000000000000000000002"
F_PERS = "a1110000000000000000000000000003"
RES_ID = "b2220000000000000000000000000001"

# ---- SVG 架构图资源 ----
svg = '''<svg xmlns="http://www.w3.org/2000/svg" width="700" height="300" viewBox="0 0 700 300" font-family="-apple-system,Segoe UI,Roboto,sans-serif">
  <defs>
    <linearGradient id="bar" x1="0" y1="0" x2="1" y2="0">
      <stop offset="0" stop-color="#7c4dff"/><stop offset="1" stop-color="#4d8bff"/>
    </linearGradient>
  </defs>
  <rect width="700" height="300" rx="18" fill="#0f1117"/>
  <rect x="0" y="0" width="700" height="6" fill="url(#bar)"/>
  <text x="36" y="56" fill="#e8eaf0" font-size="24" font-weight="700">Jasper · 架构</text>
  <text x="36" y="84" fill="#8b90a0" font-size="14">本地 Rust 服务 + 浏览器 SPA，无 Electron</text>
  <g>
    <rect x="36"  y="120" width="180" height="110" rx="12" fill="#1a1d28" stroke="#7c4dff" stroke-width="1.5"/>
    <text x="126" y="166" fill="#fff" font-size="17" font-weight="600" text-anchor="middle">浏览器 SPA</text>
    <text x="126" y="190" fill="#9aa0b4" font-size="13" text-anchor="middle">Svelte 5 · CodeMirror</text>
  </g>
  <g>
    <rect x="260" y="120" width="180" height="110" rx="12" fill="#1a1d28" stroke="#4d8bff" stroke-width="1.5"/>
    <text x="350" y="166" fill="#fff" font-size="17" font-weight="600" text-anchor="middle">Rust 服务</text>
    <text x="350" y="190" fill="#9aa0b4" font-size="13" text-anchor="middle">axum · ~10MB 常驻</text>
  </g>
  <g>
    <rect x="484" y="120" width="180" height="110" rx="12" fill="#1a1d28" stroke="#39c0a0" stroke-width="1.5"/>
    <text x="574" y="160" fill="#fff" font-size="17" font-weight="600" text-anchor="middle">存储</text>
    <text x="574" y="184" fill="#9aa0b4" font-size="13" text-anchor="middle">本地文件夹</text>
    <text x="574" y="204" fill="#9aa0b4" font-size="13" text-anchor="middle">/ WebDAV</text>
  </g>
  <path d="M216 175 L260 175" stroke="#6b7080" stroke-width="2" marker-end="url(#a)"/>
  <path d="M440 175 L484 175" stroke="#6b7080" stroke-width="2" marker-end="url(#a)"/>
  <defs><marker id="a" markerWidth="9" markerHeight="9" refX="7" refY="3" orient="auto"><path d="M0,0 L7,3 L0,6 Z" fill="#6b7080"/></marker></defs>
  <text x="238" y="166" fill="#6b7080" font-size="11" text-anchor="middle">HTTP</text>
  <text x="462" y="166" fill="#6b7080" font-size="11" text-anchor="middle">读写</text>
</svg>'''
svg_bytes = svg.encode("utf-8")
with open(os.path.join(RES, RES_ID), "wb") as f:
    f.write(svg_bytes)
write(f"{RES_ID}.md", resource(RES_ID, "架构图.svg", "image/svg+xml", "svg", len(svg_bytes), "2026-06-20T08:00:00.000Z"))

# ---- 笔记本 ----
write(f"{F_TECH}.md", folder(F_TECH, "技术笔记 · Tech", "", "2026-06-10T09:00:00.000Z"))
write(f"{F_WORK}.md", folder(F_WORK, "工作 · Work", "", "2026-06-11T09:00:00.000Z"))
write(f"{F_PERS}.md", folder(F_PERS, "个人 · Personal", "", "2026-06-12T09:00:00.000Z"))

# ---- 笔记 ----
showcase = f"""Jasper 是一个**轻量、可读可写**的 Joplin 兼容客户端。下面展示渲染能力。

![架构图](:/{RES_ID})

## 它能做什么
- 直接读写本地文件夹或 **WebDAV** 上的 Joplin 同步库
- Markdown 实时渲染：代码高亮、表格、数学公式、任务清单
- 资源/图片：粘贴、拖拽、附件上传，并可在面板里管理

> 后端常驻内存约 10MB，启动快、跨平台、不依赖 Electron。

### 代码高亮
```rust
fn main() {{
    let greeting = "你好, Jasper";
    println!("{{greeting}}");
}}
```

### 表格
| 数据源 | 读 | 写 |
|--------|----|----|
| 本地文件夹 | ✅ | ✅ |
| WebDAV | ✅ | ✅ |

### 任务清单
- [x] 本地 / WebDAV 读取
- [x] 增量缓存
- [x] 资源上传与管理
- [ ] 标签视图

### 数学公式
行内 $E = mc^2$，独立公式：

$$\\int_0^\\infty e^{{-x}}\\,dx = 1$$
"""
write("c1110000000000000000000000000001.md",
      note("c1110000000000000000000000000001", F_TECH, "✨ 功能展示 Feature Showcase", showcase, "2026-06-28T15:20:00.000Z"))

rust_note = """所有权三条规则：

1. 每个值有且仅有一个**所有者**
2. 同一时刻只能有一个所有者
3. 所有者离开作用域，值被丢弃

```rust
fn longest<'a>(x: &'a str, y: &'a str) -> &'a str {
    if x.len() > y.len() { x } else { y }
}
```

借用检查器在**编译期**保证内存安全，无需 GC。
"""
write("c1110000000000000000000000000002.md",
      note("c1110000000000000000000000000002", F_TECH, "Rust 所有权与借用", rust_note, "2026-06-26T11:00:00.000Z"))

plan_note = """## 目标
- 完成增量缓存与资源管理
- WebDAV 端到端联调

| 里程碑 | 状态 |
|--------|------|
| 增量缓存 | ✅ 已完成 |
| 资源上传 | ✅ 已完成 |
| 鉴权 | 🚧 进行中 |
"""
write("c1110000000000000000000000000003.md",
      note("c1110000000000000000000000000003", F_WORK, "Q3 项目计划", plan_note, "2026-06-29T09:30:00.000Z"))

meeting_note = """**时间**：2026-06-29 10:00
**参会**：全体

### 结论
- 增量缓存上线，WebDAV 启动只拉变化项
- 资源面板支持孤儿清理

### 待办
- [ ] 补充 README
- [ ] 准备演示截图
"""
write("c1110000000000000000000000000004.md",
      note("c1110000000000000000000000000004", F_WORK, "周会纪要 2026-06-29", meeting_note, "2026-06-29T10:40:00.000Z"))

reading_note = """- [x] 《Rust 程序设计语言》
- [x] 《深入理解计算机系统》
- [ ] 《数据密集型应用系统设计》
- [ ] 《Designing Data-Intensive Applications》

> 读完做一页纸笔记。
"""
write("c1110000000000000000000000000005.md",
      note("c1110000000000000000000000000005", F_PERS, "📚 读书清单", reading_note, "2026-06-22T20:00:00.000Z"))

travel_note = f"""周末去了山里，信号很差但风景好。

![架构图](:/{RES_ID})

- 带了纸质书
- 拍了很多照片
- 回来把笔记同步回 Joplin
"""
write("c1110000000000000000000000000006.md",
      note("c1110000000000000000000000000006", F_PERS, "🏔 旅行碎记", travel_note, "2026-06-15T18:00:00.000Z"))

print("生成完成:", ROOT)
print("文件数:", len([n for n in os.listdir(ROOT) if n.endswith('.md')]), ".md +", len(os.listdir(RES)), "资源")
