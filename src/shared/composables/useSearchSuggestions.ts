import { ref, watch, type Ref } from "vue";

export function useSearchSuggestions(
  search: Ref<string>,
  suggestions: Readonly<Ref<readonly { name: string }[]>>,
  chooseSuggestion: (name: string) => void,
  commitSearch: () => void,
  searchInput: Readonly<Ref<HTMLInputElement | null>>,
) {
  const searchFocused = ref(false);

  const suggestionIndex = ref(-1);

  watch(search, () => {
    suggestionIndex.value = -1;
  });

  function chooseSearchSuggestion(name: string) {
    chooseSuggestion(name);
    searchFocused.value = false;
  }

  function searchKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      searchFocused.value = false;
    } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      searchFocused.value = true;

      const count = suggestions.value.length;

      suggestionIndex.value = count
        ? (suggestionIndex.value +
            (event.key === "ArrowDown" ? 1 : count - 1) +
            count) %
          count
        : -1;
    } else if (event.key === "Enter") {
      event.preventDefault();

      const suggestion = searchFocused.value
        ? suggestions.value[suggestionIndex.value]
        : undefined;

      if (suggestion) chooseSearchSuggestion(suggestion.name);
      else {
        commitSearch();
        searchFocused.value = false;
      }
    }
  }

  return {
    searchInput,
    searchFocused,
    suggestionIndex,
    chooseSearchSuggestion,
    searchKeydown,
  };
}
