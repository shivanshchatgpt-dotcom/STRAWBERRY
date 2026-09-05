import { useCallback, useState } from "react";

/**
 * Sub-section state for the Memory area.
 *
 * The Memory panel keeps its own sub-navigation (overview, search,
 * detail, etc.) separate from the top-level View so that "Memory →
 * Detail" is a stable sub-route.
 */
export type MemorySubView =
  | "overview"
  | "search"
  | "detail"
  | "create"
  | "edit"
  | "images"
  | "credentials"
  | "watchers"
  | "activity";

export interface MemoryNav {
  view: MemorySubView;
  memoryId: string | null;
  goOverview: () => void;
  goSearch: (q?: string) => void;
  goDetail: (id: string) => void;
  goCreate: () => void;
  goEdit: (id: string) => void;
  goImages: () => void;
  goCredentials: () => void;
  goWatchers: () => void;
  goActivity: () => void;
}

export function useMemoryNav(): MemoryNav {
  const [view, setView] = useState<MemorySubView>("overview");
  const [memoryId, setMemoryId] = useState<string | null>(null);

  const goOverview = useCallback(() => {
    setView("overview");
    setMemoryId(null);
  }, []);
  const goSearch = useCallback((_q?: string) => {
    setView("search");
    setMemoryId(null);
  }, []);
  const goDetail = useCallback((id: string) => {
    setView("detail");
    setMemoryId(id);
  }, []);
  const goCreate = useCallback(() => {
    setView("create");
    setMemoryId(null);
  }, []);
  const goEdit = useCallback((id: string) => {
    setView("edit");
    setMemoryId(id);
  }, []);
  const goImages = useCallback(() => {
    setView("images");
    setMemoryId(null);
  }, []);
  const goCredentials = useCallback(() => {
    setView("credentials");
    setMemoryId(null);
  }, []);
  const goWatchers = useCallback(() => {
    setView("watchers");
    setMemoryId(null);
  }, []);
  const goActivity = useCallback(() => {
    setView("activity");
    setMemoryId(null);
  }, []);

  return {
    view,
    memoryId,
    goOverview,
    goSearch,
    goDetail,
    goCreate,
    goEdit,
    goImages,
    goCredentials,
    goWatchers,
    goActivity,
  };
}
