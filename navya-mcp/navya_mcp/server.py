# Navya Studio FastMCP Server (Phase 13).
#
# Complete FastMCP server with all studio tools registered via @mcp.tool,
# each a thin httpx proxy to the control server.

from fastmcp import FastMCP, Context
import httpx
import os

# Control server URL and token from environment
CONTROL_URL = os.environ.get("NAVYA_CONTROL_URL", "http://127.0.0.1:8080")
CONTROL_TOKEN = os.environ.get("NAVYA_CONTROL_TOKEN")

mcp = FastMCP("navya-studio-tools")

def call_tool_endpoint(tool_name: str, payload: dict) -> dict:
    """Call a tool endpoint on the control server."""
    if not CONTROL_TOKEN:
        raise ValueError("NAVYA_CONTROL_TOKEN not set")
    
    url = f"{CONTROL_URL}/tool/{tool_name}"
    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {CONTROL_TOKEN}",
    }
    
    with httpx.Client() as client:
        response = client.post(url, json=payload, headers=headers)
        response.raise_for_status()
        return response.json()

@mcp.tool()
async def generate_image(ctx: Context, project_id: str, prompt: str, composition_id: str | None = None, model: str | None = None, size: str | None = None) -> dict:
    """Generate an image via Navya Cloud or local sd-server."""
    payload = {"project_id": project_id, "prompt": prompt, "composition_id": composition_id, "model": model, "size": size}
    ctx.report_progress("generating image...")
    return call_tool_endpoint("generate_image", payload)

@mcp.tool()
async def render_to_video(ctx: Context, project_id: str, composition_id: str, quality: str, target: str) -> dict:
    """Queue a video render via the HyperFrames render sidecar."""
    payload = {"project_id": project_id, "composition_id": composition_id, "quality": quality, "target": target}
    ctx.report_progress("queuing render...")
    return call_tool_endpoint("render_to_video", payload)

@mcp.tool()
async def list_local_models(ctx: Context) -> dict:
    """List available cloud and local models."""
    return call_tool_endpoint("list_local_models", {})

@mcp.tool()
async def set_generation_source(ctx: Context, source: str) -> dict:
    """Set the generation source (cloud vs local)."""
    payload = {"source": source}
    return call_tool_endpoint("set_generation_source", payload)

@mcp.tool()
async def get_project_state(ctx: Context) -> dict:
    """Get current project state from the store."""
    return call_tool_endpoint("get_project_state", {})

@mcp.tool()
async def snapshot(ctx: Context, timecode_ms: int) -> dict:
    """Snapshot a frame at timecode t (ms) from the current composition."""
    payload = {"timecode_ms": timecode_ms}
    return call_tool_endpoint("snapshot", payload)

@mcp.tool()
async def open_in_folder(ctx: Context, path: str) -> dict:
    """Reveal a file or folder in the OS file manager."""
    payload = {"path": path}
    return call_tool_endpoint("open_in_folder", payload)

if __name__ == "__main__":
    mcp.run()
