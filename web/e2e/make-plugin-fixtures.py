#!/usr/bin/env python3
"""生成 e2e 用的插件包夹具（fixtures/*.jplug，zip 已提交入库，CI 无需重跑本脚本）。

- theme.jplug     零代码主题插件：安装即自动启用，验证主题贡献 → ThemePicker → 卸载回落。
- caps-demo.jplug 含 [backend] + capabilities 的插件：验证 consent 弹窗（保持禁用路径，
                  wasm 不会被编译执行，故用最小合法空模块即可）。
"""

import io
import pathlib
import zipfile

OUT = pathlib.Path(__file__).parent / 'fixtures'

THEME_MANIFEST = """id = "e2e-theme"
name = "E2E Theme"
version = "1.0.0"
apiVersion = "0.1"
description = "e2e 夹具：零代码主题"

[[contributes.theme]]
id = "e2e-lime"
name = "E2E Lime"
base = "light"
css = "assets/e2e-lime.css"
"""

THEME_CSS = ":root[data-theme='e2e-lime'] { --accent: #32cd32; --accent-soft: rgba(50, 205, 50, 0.18); }\n"

CAPS_MANIFEST = """id = "caps-demo"
name = "Caps Demo"
version = "1.0.0"
apiVersion = "0.2"
description = "e2e 夹具：能力授权确认"

[backend]
wasm = "plugin.wasm"
capabilities = ["notes:read", "host:http"]
"""

# 最小合法 wasm 模块（仅 magic + version；安装不编译 wasm，enable 才会）
EMPTY_WASM = b"\x00asm\x01\x00\x00\x00"


def write_zip(name: str, entries: dict[str, bytes]) -> None:
	buf = io.BytesIO()
	with zipfile.ZipFile(buf, 'w', zipfile.ZIP_DEFLATED) as z:
		for path, data in entries.items():
			z.writestr(path, data)
	out = OUT / name
	out.write_bytes(buf.getvalue())
	print(f'{out} ({out.stat().st_size} bytes)')


def main() -> None:
	OUT.mkdir(exist_ok=True)
	write_zip('theme.jplug', {
		'manifest.toml': THEME_MANIFEST.encode(),
		'assets/e2e-lime.css': THEME_CSS.encode(),
	})
	write_zip('caps-demo.jplug', {
		'manifest.toml': CAPS_MANIFEST.encode(),
		'plugin.wasm': EMPTY_WASM,
	})


if __name__ == '__main__':
	main()
