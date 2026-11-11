export function LogoMark({ size = 18 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      aria-hidden
    >
      <rect x="3" y="3" width="18" height="18" rx="5" stroke="currentColor" />
      <path
        d="M7.5 9.25h9M7.5 12h5.5M7.5 14.75h7"
        stroke="currentColor"
        strokeLinecap="round"
      />
    </svg>
  );
}
