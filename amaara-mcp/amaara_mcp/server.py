# Amaara Studio FastMCP Server (Phase 13).
#
# Complete FastMCP server with all studio tools registered via @mcp.tool,
# each a thin httpx proxy to the control server.

from fastmcp import FastMCP, Context
from fastmcp.utilities.types import Image
import httpx
import os

# Control server URL and token from environment
CONTROL_URL = os.environ.get("AMAARA_CONTROL_URL", "http://127.0.0.1:8080")
CONTROL_TOKEN = os.environ.get("AMAARA_CONTROL_TOKEN")

mcp = FastMCP("amaara-studio-tools")

async def call_tool_endpoint(tool_name: str, payload: dict) -> dict:
    """Call a tool endpoint on the control server."""
    if not CONTROL_TOKEN:
        raise ValueError("AMAARA_CONTROL_TOKEN not set")

    url = f"{CONTROL_URL}/tool/{tool_name}"
    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {CONTROL_TOKEN}",
    }

    async with httpx.AsyncClient(timeout=300) as client:
        response = await client.post(url, json=payload, headers=headers)
        response.raise_for_status()
        return response.json()

async def _report_progress(ctx: Context, progress: int, total: int, message: str):
    """Report progress in a way that is safe if the transport does not support it."""
    try:
        await ctx.report_progress(progress, total, message)
    except Exception:
        pass

@mcp.tool()
async def generate_image(
    project_id: str,
    prompt: str,
    ctx: Context,
    composition_id: str | None = None,
    model: str | None = None,
    size: str | None = None,
    n: int = 1,
) -> dict:
    """Generate an image via Amaara Cloud or local sd-server."""
    await _report_progress(ctx, 0, 100, "queuing image generation")
    payload = {
        "project_id": project_id,
        "prompt": prompt,
        "composition_id": composition_id,
        "model": model,
        "size": size,
        "n": n,
    }
    result = await call_tool_endpoint("generate_image", payload)
    await _report_progress(ctx, 100, 100, "image generated")
    return result

@mcp.tool()
async def render_to_video(
    project_id: str,
    composition_id: str,
    quality: str,
    target: str,
    ctx: Context,
) -> dict:
    """Queue a video render via the HyperFrames render sidecar."""
    await _report_progress(ctx, 0, 100, "queuing render")
    payload = {
        "project_id": project_id,
        "composition_id": composition_id,
        "quality": quality,
        "target": target,
    }
    result = await call_tool_endpoint("render_to_video", payload)
    await _report_progress(ctx, 100, 100, "render queued")
    return result

@mcp.tool()
async def list_local_models(ctx: Context) -> dict:
    """List available cloud and local models."""
    return await call_tool_endpoint("list_local_models", {})

@mcp.tool()
async def set_generation_source(source: str, ctx: Context) -> dict:
    """Set the generation source (cloud vs local)."""
    payload = {"source": source}
    return await call_tool_endpoint("set_generation_source", payload)

@mcp.tool()
async def get_project_state(ctx: Context) -> dict:
    """Get current project state from the store."""
    return await call_tool_endpoint("get_project_state", {})

@mcp.tool()
async def snapshot(timecode_ms: int, ctx: Context) -> dict:
    """Snapshot a frame at timecode t (ms) from the current composition."""
    payload = {"timecode_ms": timecode_ms}
    return await call_tool_endpoint("snapshot", payload)

@mcp.tool()
async def open_in_folder(path: str, ctx: Context) -> dict:
    """Reveal a file or folder in the OS file manager."""
    payload = {"path": path}
    return await call_tool_endpoint("open_in_folder", payload)

if __name__ == "__main__":
    mcp.run()
