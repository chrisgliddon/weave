// Colour theme: dark by default, with light and follow-system options.
(() => {
  const STORAGE_KEY = "weave-theme";
  const root = document.documentElement;
  const media = window.matchMedia("(prefers-color-scheme: light)");
  const inputs = document.querySelectorAll('input[name="theme"]');

  const preference = () => {
    const current = root.dataset.themePreference;
    return current === "light" || current === "system" ? current : "dark";
  };

  const paint = (value) => {
    const dark = value === "system" ? !media.matches : value === "dark";
    root.dataset.themePreference = value;
    root.dataset.theme = dark ? "dark" : "light";
    for (const input of inputs) {
      input.checked = input.value === value;
    }
  };

  const remember = (value) => {
    try {
      localStorage.setItem(STORAGE_KEY, value);
    } catch (error) {
      // A blocked storage API should not stop the theme from applying.
    }
  };

  for (const input of inputs) {
    input.addEventListener("change", () => {
      if (!input.checked) {
        return;
      }
      remember(input.value);
      paint(input.value);
    });
  }

  media.addEventListener("change", () => {
    if (preference() === "system") {
      paint("system");
    }
  });

  paint(preference());
})();
