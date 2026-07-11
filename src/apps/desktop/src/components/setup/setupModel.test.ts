import { describe, expect, it } from "vitest";
import type { ControlSnapshot, ProviderInstanceItem } from "../../lib/desktop";
import { buildSetupProgress } from "./setupModel";

const control: ControlSnapshot = {
  workspaceId: "default", workspaceName: "API", recoveredFromBackup: false,
  requiredSecretCount: 0, missingSecretCount: 0, providerCount: 0, publisherCount: 0,
  supervisor: { publishers: [] }, notifications: [],
};

describe("guided setup progress", () => {
  it("starts with Provider for an empty workspace", () => {
    const result = buildSetupProgress(control, []);
    expect(result.current).toBe("provider");
    expect(result.completed).toBe(0);
  });

  it("does not claim inspection completion without a real report", () => {
    const provider: ProviderInstanceItem = { id: "main", providerId: "openai", name: "OpenAI", endpointOverride: null, enabled: true, secretFields: ["api_key"], inspectionAvailable: false };
    const result = buildSetupProgress({ ...control, providerCount: 1, requiredSecretCount: 1 }, [provider]);
    expect(result.steps.find((step) => step.id === "provider")?.complete).toBe(true);
    expect(result.current).toBe("inspection");
  });

  it("reaches the call step only after a Publisher exists", () => {
    const provider: ProviderInstanceItem = { id: "main", providerId: "openai", name: "OpenAI", endpointOverride: null, enabled: true, secretFields: ["api_key"], inspectionAvailable: true };
    const result = buildSetupProgress({ ...control, providerCount: 1, publisherCount: 1, supervisor: { publishers: [{ id: "local", status: "stopped" }] } }, [provider]);
    expect(result.completed).toBe(4);
    expect(result.steps.every((step) => step.complete)).toBe(true);
  });
});
