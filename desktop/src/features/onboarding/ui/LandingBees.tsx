/**
 * Landing background: ghosted Omni marks drifting across the shell. Replaces
 * the upstream bee field with the product's own glyph — static, GPU-cheap,
 * and theme-aware via low-opacity foreground tokens.
 */

const MARKS = [
  { top: "-12%", left: "-6%", size: 560, opacity: 0.05, rotate: -12 },
  { top: "38%", left: "68%", size: 720, opacity: 0.04, rotate: 18 },
  { top: "72%", left: "8%", size: 460, opacity: 0.05, rotate: 32 },
  { top: "-18%", left: "58%", size: 380, opacity: 0.035, rotate: -24 },
  { top: "55%", left: "-14%", size: 520, opacity: 0.045, rotate: -6 },
];

export function LandingBees() {
  return (
    <div
      aria-hidden="true"
      className="pointer-events-none absolute inset-0 overflow-hidden"
      data-testid="landing-omni-field"
    >
      {MARKS.map((m, i) => (
        <svg
          key={i}
          className="absolute text-foreground"
          style={{
            top: m.top,
            left: m.left,
            width: m.size,
            height: m.size,
            opacity: m.opacity,
            transform: `rotate(${m.rotate}deg)`,
          }}
          viewBox="0 0 596 596"
          fill="currentColor"
        >
          <circle cx="298" cy="298" r="175" stroke="currentColor" strokeWidth="32" fill="none" />
          <line x1="298" y1="298" x2="298" y2="123" stroke="currentColor" strokeWidth="32" />
          <line x1="298" y1="298" x2="449.55" y2="385.5" stroke="currentColor" strokeWidth="32" />
          <line x1="298" y1="298" x2="146.45" y2="385.5" stroke="currentColor" strokeWidth="32" />
          <circle cx="298" cy="123" r="60" fill="currentColor" />
          <circle cx="449.55" cy="385.5" r="60" fill="currentColor" />
          <circle cx="146.45" cy="385.5" r="60" fill="currentColor" />
          <circle cx="298" cy="298" r="74" fill="currentColor" />
        </svg>
      ))}
    </div>
  );
}
