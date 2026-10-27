const VIDEO_SRC = "/landing/omni-login-glass-loop.mp4";

/**
 * Landing background: the Omni glass loop, full-bleed behind the login gate.
 * Replaces the static mark field on the machine onboarding gate. The loop is
 * muted, inline, and covers the frame; a theme-aware scrim keeps the
 * wordmark and CTAs readable on both schemes. `prefers-reduced-motion` gets
 * a still first frame instead of the animation.
 */
export function LandingVideoBackdrop() {
  return (
    <div
      aria-hidden="true"
      className="pointer-events-none absolute inset-0 overflow-hidden"
      data-testid="landing-video-backdrop"
    >
      <video
        autoPlay
        className="h-full w-full object-cover motion-reduce:hidden"
        disablePictureInPicture
        loop
        muted
        playsInline
        src={VIDEO_SRC}
        tabIndex={-1}
      />
      {/* Scrim: darker at the content column, lighter at the edges. */}
      <div className="absolute inset-0 bg-background/40 motion-reduce:bg-background/70" />
    </div>
  );
}
