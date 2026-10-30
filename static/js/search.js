// Client-side documentation search over the generated search index.
(() => {
  const input = document.querySelector("[data-search-input]");
  const panel = document.getElementById("search-results");
  if (!input || !panel) {
    return;
  }

  let index = null;
  let pending = null;

  const load = () => {
    if (index) {
      return Promise.resolve(index);
    }
    if (!pending) {
      const url = new URL("searchindex.json", document.baseURI);
      pending = fetch(url)
        .then((response) => (response.ok ? response.json() : []))
        .catch(() => [])
        .then((entries) => {
          index = Array.isArray(entries) ? entries : [];
          return index;
        });
    }
    return pending;
  };

  const close = () => {
    panel.hidden = true;
    panel.replaceChildren();
    input.setAttribute("aria-expanded", "false");
  };

  const snippet = (text, needle) => {
    const at = text.toLowerCase().indexOf(needle);
    if (at < 0) {
      return text.slice(0, 120);
    }
    const start = Math.max(0, at - 48);
    return `${start > 0 ? "…" : ""}${text.slice(start, start + 130)}…`;
  };

  const render = (entries, query) => {
    panel.replaceChildren();
    if (entries.length === 0) {
      const empty = document.createElement("p");
      empty.className = "search-empty";
      empty.textContent = "No matching pages.";
      panel.append(empty);
    } else {
      for (const entry of entries) {
        const link = document.createElement("a");
        link.className = "search-result";
        link.href = entry.url;
        const title = document.createElement("strong");
        title.textContent = entry.title;
        const body = document.createElement("span");
        body.textContent = snippet(entry.text || "", query);
        link.append(title, body);
        panel.append(link);
      }
    }
    panel.hidden = false;
    input.setAttribute("aria-expanded", "true");
  };

  input.addEventListener("input", async () => {
    const query = input.value.trim().toLowerCase();
    if (query.length < 2) {
      close();
      return;
    }
    const entries = await load();
    render(
      entries
        .filter(
          (entry) =>
            entry.title.toLowerCase().includes(query) ||
            (entry.text || "").toLowerCase().includes(query),
        )
        .slice(0, 10),
      query,
    );
  });

  input.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      close();
      input.blur();
    }
  });

  document.addEventListener("keydown", (event) => {
    if (event.key === "/" && document.activeElement !== input) {
      const tag = document.activeElement?.tagName;
      if (tag !== "INPUT" && tag !== "TEXTAREA") {
        event.preventDefault();
        input.focus();
      }
    }
  });

  document.addEventListener("click", (event) => {
    if (!panel.contains(event.target) && event.target !== input) {
      close();
    }
  });
})();
