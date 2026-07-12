import { describe, expect, it } from "vitest";
import { isRecoverableReactCacheError, nextReactRecoveryAttempt } from "./ClientErrorBoundary";

describe("React production recovery", () => {
  it("recognizes the production and development hook-order diagnostics", () => {
    expect(isRecoverableReactCacheError("Minified React error #310; visit https://react.dev/errors/310")).toBe(true);
    expect(isRecoverableReactCacheError("Rendered more hooks than during the previous render")).toBe(true);
  });

  it("does not reload for unrelated application errors", () => {
    expect(isRecoverableReactCacheError("LocalGateway is unavailable")).toBe(false);
  });

  it("allows three recovery reloads and then exposes the diagnostic boundary", () => {
    const error = "Minified React error #310";
    expect(nextReactRecoveryAttempt(error, null)).toBe(1);
    expect(nextReactRecoveryAttempt(error, "1")).toBe(2);
    expect(nextReactRecoveryAttempt(error, "2")).toBe(3);
    expect(nextReactRecoveryAttempt(error, "3")).toBeNull();
    expect(nextReactRecoveryAttempt("unrelated", "0")).toBeNull();
  });
});
