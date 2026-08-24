/**
 * Navya Studio App (Phase 6 shell + Phase 0 branding).
 */

import { useEffect } from "react";
import { TopBar, LeftRail, ChatView, RightRail, StatusStrip, TabBar } from "./components/Shell";

export default function App() {
  useEffect(() => {
    // Reserved for first-launch onboarding (Phase 16). No-op now so the window
    // renders the shell immediately.
  }, []);

  return (
    <div className="flex flex-col h-screen bg-studio-900 text-slate-100">
      <TopBar />
      <TabBar />
      <div className="flex flex-1 overflow-hidden">
        <LeftRail />
        <ChatView />
        <RightRail />
      </div>
      <StatusStrip />
    </div>
  );
}
