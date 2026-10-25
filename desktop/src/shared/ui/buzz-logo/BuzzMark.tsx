/**
 * The Omni mark as a plain static SVG — no SMIL, no scripting, no animation
 * machinery. Hub-and-spokes glyph rendered in `currentColor`, so it follows
 * the active theme accent and paints complete on the very first frame
 * regardless of animation support.
 */
export function BuzzMark({ className }: { className?: string }) {
  return (
    <svg
      aria-hidden="true"
      className={["buzz-mark", className].filter(Boolean).join(" ")}
      viewBox="0 0 596 596"
      fill="currentColor"
    >
      <circle
        cx="298"
        cy="298"
        r="175"
        stroke="currentColor"
        strokeWidth="32"
        fill="none"
      />
      <line
        x1="298"
        y1="298"
        x2="298"
        y2="123"
        stroke="currentColor"
        strokeWidth="32"
      />
      <line
        x1="298"
        y1="298"
        x2="449.55"
        y2="385.5"
        stroke="currentColor"
        strokeWidth="32"
      />
      <line
        x1="298"
        y1="298"
        x2="146.45"
        y2="385.5"
        stroke="currentColor"
        strokeWidth="32"
      />
      <circle cx="298" cy="123" r="60" fill="currentColor" />
      <circle cx="449.55" cy="385.5" r="60" fill="currentColor" />
      <circle cx="146.45" cy="385.5" r="60" fill="currentColor" />
      <circle cx="298" cy="298" r="74" fill="currentColor" />
    </svg>
  );
}
