import { useAppStore } from "../../store/appStore";

export function Breadcrumb() {
  const breadcrumb = useAppStore((s) => s.breadcrumb);
  const navigateBreadcrumb = useAppStore((s) => s.navigateBreadcrumb);

  return (
    <nav className="crumbs" aria-label="Breadcrumb">
      <button className="crumb" onClick={() => navigateBreadcrumb(null)}>
        Home
      </button>
      {breadcrumb.map((item, i) => (
        <span key={`${item.kind}-${item.id}`} style={{ display: "contents" }}>
          <span className="crumb-sep">›</span>
          <button
            className={`crumb${i === breadcrumb.length - 1 ? " current" : ""}`}
            disabled={i === breadcrumb.length - 1 && item.kind !== "root"}
            onClick={() => navigateBreadcrumb(item)}
          >
            {item.label}
          </button>
        </span>
      ))}
    </nav>
  );
}
