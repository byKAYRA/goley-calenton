import asyncio
import os
import sys
import traceback

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client


async def probe(
    name: str,
    command: str,
    args: list[str],
    env: dict[str, str] | None = None,
    cwd: str | None = None,
    timeout: int = 15,
) -> None:
    params = StdioServerParameters(command=command, args=args, env=env, cwd=cwd)
    try:
        async with stdio_client(params) as (read, write):
            async with ClientSession(read, write) as session:
                info = await asyncio.wait_for(session.initialize(), timeout=timeout)
                tools = await asyncio.wait_for(session.list_tools(), timeout=timeout)
                server_info = getattr(info, "server_info", None) or getattr(info, "serverInfo", None)
                server_name = getattr(server_info, "name", "unknown")
                print(f"{name}: OK | server={server_name} | tools={len(tools.tools)}")
    except Exception as exc:
        print(f"{name}: FAIL | {type(exc).__name__}: {exc}")
        traceback.print_exception(exc)


async def main() -> None:
    root = r"<ToolsPath>\GoleyRE"
    await probe(
        "frida",
        rf"{root}\MCP\frida-mcp-main\.venv\Scripts\frida-mcp.exe",
        [],
    )
    await probe(
        "ghidra-bridge",
        rf"{root}\MCP\GhidraMCP-current\.venv\Scripts\python.exe",
        [
            rf"{root}\MCP\GhidraMCP-current\bridge_mcp_ghidra.py",
            "--ghidra-server",
            "http://127.0.0.1:8080/",
        ],
    )
    wire_env = dict(os.environ)
    wire_env["PATH"] = rf"{root}\Wireshark-4.6.8;" + wire_env.get("PATH", "")
    await probe(
        "wiremcp",
        r"C:\Program Files\nodejs\node.exe",
        [rf"{root}\MCP\WireMCP-main\index.js"],
        wire_env,
    )
    await probe(
        "idalib",
        r"%USERPROFILE%\AppData\Local\Microsoft\WinGet\Packages\astral-sh.uv_Microsoft.Winget.Source_8wekyb3d8bbwe\uv.exe",
        ["run", "idalib-mcp", "--stdio"],
        cwd=r"%USERPROFILE%\.codex\plugins\cache\mrexodia\ida-pro-mcp\0.1.0",
        timeout=90,
    )


if __name__ == "__main__":
    if sys.platform == "win32":
        asyncio.set_event_loop_policy(asyncio.WindowsProactorEventLoopPolicy())
    asyncio.run(main())
