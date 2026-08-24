import { useEffect } from "react";

/**
 * Phase 0 scaffold: an empty, branded page. The window shell (top bar / rails)
 * is added in Phase 6; the onboarding screen in Phase 16.
 */
export default function App() {
  useEffect(() => {
    // Reserved for first-launch onboarding (Phase 16). No-op now so the window
    // renders a title immediately at Gate 0.
  }, []);

  return (
    <main className="flex min-h-screen items-center justify-center bg-[#0a1a2f] text-slate-100">
      <div className="text-center">
        <p className="mb-4 text-sm tracking-widest text-violet-300">AI CONTENT-creation STUDIO</p>
        <h1 className="text-4xl font-bold">Navya Studio</h1>
      </div>
    </main>
  );
}
