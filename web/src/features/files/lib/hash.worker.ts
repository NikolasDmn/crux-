// SHA-256 of an upload, off the main thread (frontend-spec 5.1).
const ctx = self as unknown as {
  onmessage: ((e: MessageEvent<File>) => void) | null;
  postMessage: (m: { hex?: string; error?: string }) => void;
};

ctx.onmessage = async (e) => {
  try {
    const digest = await crypto.subtle.digest("SHA-256", await e.data.arrayBuffer());
    const hex = [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("");
    ctx.postMessage({ hex });
  } catch (err) {
    ctx.postMessage({ error: String(err) });
  }
};
