import { onMounted, onUnmounted, ref, watch } from "vue";
import type { CommandError, ModHit, ModSearch } from "../api";

// A search field's list: asks again 300 ms after typing stops, and a slow answer never replaces a newer one.
export function useSearch(fetch: (query: string, offset: number) => Promise<ModSearch>) {
  const query = ref("");
  const hits = ref<ModHit[]>([]);
  const more = ref(false);
  const loading = ref(false);
  const failure = ref<CommandError | null>(null);
  let latest = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;

  async function load(offset = 0) {
    clearTimeout(timer);
    const mine = ++latest;
    loading.value = true;
    failure.value = null;
    if (!offset) hits.value = [];
    try {
      const found = await fetch(query.value.trim(), offset);
      if (mine !== latest) return;
      hits.value = offset ? [...hits.value, ...found.hits] : found.hits;
      more.value = found.more;
    } catch (e) {
      if (mine === latest) failure.value = e as CommandError;
    } finally {
      if (mine === latest) loading.value = false;
    }
  }

  watch(query, () => {
    clearTimeout(timer);
    timer = setTimeout(() => load(), 300);
  });
  onMounted(() => load());
  onUnmounted(() => clearTimeout(timer));
  return { query, hits, more, loading, failure, load };
}
