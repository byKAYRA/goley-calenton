import asyncio

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client


async def main() -> None:
    root = r"<ToolsPath>\GoleyRE\MCP\GhidraMCP-current"
    params = StdioServerParameters(
        command=rf"{root}\.venv\Scripts\python.exe",
        args=[
            rf"{root}\bridge_mcp_ghidra.py",
            "--ghidra-server",
            "http://127.0.0.1:8080/",
        ],
    )
    async with stdio_client(params) as (read, write):
        async with ClientSession(read, write) as session:
            info = await session.initialize()
            tools = await session.list_tools()
            address = await session.call_tool("get_current_address", {})
            imports = await session.call_tool("list_imports", {"offset": 0, "limit": 3})
            server_info = getattr(info, "server_info", None) or getattr(info, "serverInfo", None)
            print(f"server={server_info.name}")
            print(f"tools={len(tools.tools)}")
            print(f"current_address={address.content[0].text}")
            print(f"imports={imports.content[0].text}")


if __name__ == "__main__":
    asyncio.run(main())
