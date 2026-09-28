type Theme = 'light' | 'dark'

const THEME_KEY = 'daily_paper_theme'

function getInitialTheme(): Theme {
  const saved = localStorage.getItem(THEME_KEY) as Theme | null
  if (saved === 'light' || saved === 'dark') return saved
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
}

let theme = $state<Theme>(getInitialTheme())

export function getTheme(): Theme {
  return theme
}

export function toggleTheme(): void {
  theme = theme === 'dark' ? 'light' : 'dark'
  localStorage.setItem(THEME_KEY, theme)
  applyTheme(theme)
}

export function initTheme(): void {
  applyTheme(theme)
}

function applyTheme(t: Theme): void {
  if (t === 'dark') {
    document.documentElement.setAttribute('data-theme', 'dark')
  } else {
    document.documentElement.removeAttribute('data-theme')
  }
}
