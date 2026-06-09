import { mount } from 'svelte'
import App from './App.svelte'
import './style.css'
import { applyTheme, loadTheme, resolveTokens } from './lib/theme'

// Apply the saved theme before mount so there's no amber flash on load.
applyTheme(resolveTokens(loadTheme()))

const app = mount(App, { target: document.getElementById('app')! })
export default app
