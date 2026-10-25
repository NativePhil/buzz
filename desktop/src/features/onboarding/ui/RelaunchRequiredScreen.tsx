import { RecoveryScreen } from "./RecoveryScreen";

export function RelaunchRequiredScreen() {
  return (
    <RecoveryScreen
      testId="relaunch-required"
      title="Restart Omni to finish recovery"
      body="Your identity was updated. Omni needs to restart so syncing and agents run under it."
    />
  );
}
