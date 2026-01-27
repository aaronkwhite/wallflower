// Get Tauri API
const { invoke } = window.__TAURI__.core;
const { getCurrentWindow } = window.__TAURI__.window;
const { save, open } = window.__TAURI__.dialog;
const { writeText } = window.__TAURI__.clipboardManager;
const { getVersion } = window.__TAURI__.app;

// Application state
let config = null;
let currentUrl = '';
let currentArticle = null;
let history = [];
let historyIndex = -1;
let isSearchMode = false;
let searchDebounceTimer = null;
let currentSortOrder = 'recent'; // recent, title, author, added
let selectedListIndex = -1; // Currently selected item in article list

// Sort icon mapping
const sortIcons = {
    recent: 'clock',
    title: 'arrow-down-a-z',
    author: 'user',
    added: 'calendar'
};

// DOM elements
const urlInput = document.getElementById('url-input');
const settingsBtn = document.getElementById('settings-btn');
const startPage = document.getElementById('start-page');
const loading = document.getElementById('loading');
const error = document.getElementById('error');
const errorMessage = document.getElementById('error-message');
const articleContainer = document.getElementById('article-container');
const settingsModal = document.getElementById('settings-modal');
const homeBtn = document.getElementById('home-btn');
const backBtn = document.getElementById('back-btn');
const forwardBtn = document.getElementById('forward-btn');
const favoriteBtn = document.getElementById('favorite-btn');
const copyMarkdownBtn = document.getElementById('copy-markdown-btn');
const saveMarkdownBtn = document.getElementById('save-markdown-btn');
const searchBtn = document.getElementById('search-btn');
const searchOverlay = document.getElementById('search-overlay');
const searchInput = document.getElementById('search-input');
const searchClearBtn = document.getElementById('search-clear-btn');
const searchCloseBtn = document.getElementById('search-close-btn');
const navBar = document.querySelector('.nav-bar');
const sortBtn = document.getElementById('sort-btn');
const sortDropdown = document.getElementById('sort-dropdown');

// Splash screen elements
const splashScreen = document.getElementById('splash-screen');
const splashVersion = document.getElementById('splash-version');

// Initialize application
document.addEventListener('DOMContentLoaded', async () => {
    // Get version early for splash screen
    let version = '0.2.5';
    try {
        version = await getVersion();
        splashVersion.textContent = `v${version}`;
        document.getElementById('version-badge').textContent = `v${version}`;
    } catch (err) {
        console.error('Failed to get version:', err);
    }

    // Apply theme early so splash matches app theme
    try {
        config = await invoke('get_config');
        applyTheme(config.theme);
        applyFontSize(config.font_size);
    } catch (err) {
        console.error('Failed to load config:', err);
        config = {
            theme: 'system',
            font_size: 17,
            max_width: 680,
            endpoints: ['https://freedium.cfd/', 'https://freedium-mirror.cfd/']
        };
    }

    setupEventListeners();
    setupKeyboardShortcuts();
    setupSwipeGestures();

    // Initialize Lucide icons
    if (window.lucide) {
        lucide.createIcons();
    }

    // Load and show start page
    await loadStartPage();

    // Fade out splash screen after minimum display time
    const minSplashTime = 1200; // Minimum time to show splash
    setTimeout(() => {
        hideSplashScreen();
    }, minSplashTime);
});

// Hide splash screen with fade animation
function hideSplashScreen() {
    if (!splashScreen) return;

    splashScreen.classList.add('fade-out');

    // Remove from DOM after fade completes
    setTimeout(() => {
        splashScreen.classList.add('hidden');
    }, 500);
}

function setupEventListeners() {
    // Window dragging
    setupWindowDrag();

    // Enter key in URL input
    urlInput.addEventListener('keydown', (e) => {
        if (e.key === 'Enter') {
            e.preventDefault();
            handleFetch();
        }
    });

    // Auto-fetch on paste
    urlInput.addEventListener('paste', () => {
        setTimeout(() => {
            const url = urlInput.value.trim();
            if (isValidMediumUrl(url)) {
                handleFetch();
            }
        }, 50);
    });

    // Clear input when focusing while viewing an article
    urlInput.addEventListener('focus', () => {
        if (!articleContainer.classList.contains('hidden')) {
            urlInput.value = '';
        }
    });

    // Navigation buttons
    homeBtn.addEventListener('click', showStartPage);
    backBtn.addEventListener('click', goBack);
    forwardBtn.addEventListener('click', goForward);

    // Favorite button
    favoriteBtn.addEventListener('click', toggleFavorite);

    // Markdown export buttons
    copyMarkdownBtn.addEventListener('click', copyAsMarkdown);
    saveMarkdownBtn.addEventListener('click', saveAsMarkdown);

    // Search functionality
    searchBtn.addEventListener('click', openSearch);
    searchCloseBtn.addEventListener('click', closeSearch);
    searchClearBtn.addEventListener('click', clearSearch);
    searchInput.addEventListener('input', handleSearchInput);
    searchInput.addEventListener('keydown', (e) => {
        if (e.key === 'Escape') {
            closeSearch();
        }
    });

    // Sort functionality
    sortBtn.addEventListener('click', (e) => {
        e.stopPropagation();
        sortDropdown.classList.toggle('open');
    });

    // Close sort dropdown when clicking outside
    document.addEventListener('click', () => {
        sortDropdown.classList.remove('open');
    });

    // Sort dropdown items
    sortDropdown.querySelectorAll('.sort-dropdown-item').forEach(item => {
        item.addEventListener('click', (e) => {
            e.stopPropagation();
            const sortOrder = item.dataset.sort;
            setSortOrder(sortOrder);
            sortDropdown.classList.remove('open');
        });
    });

    // Settings button
    settingsBtn.addEventListener('click', openSettings);

    // Settings modal buttons
    document.getElementById('close-settings-btn').addEventListener('click', closeSettings);
    document.getElementById('check-endpoints-btn').addEventListener('click', checkEndpoints);
    document.getElementById('export-db-btn').addEventListener('click', exportDatabase);
    document.getElementById('export-md-btn').addEventListener('click', exportAsMarkdown);

    // Font size slider - auto-save on change
    document.getElementById('font-size').addEventListener('input', (e) => {
        const size = e.target.value;
        document.getElementById('font-size-value').textContent = `${size}px`;
        applyFontSize(parseInt(size));
    });
    document.getElementById('font-size').addEventListener('change', (e) => {
        saveSettingsSilent({ font_size: parseInt(e.target.value) });
    });

    // Theme select - auto-save on change
    document.getElementById('theme-select').addEventListener('change', (e) => {
        applyTheme(e.target.value);
        saveSettingsSilent({ theme: e.target.value });
    });

    // Modal backdrop
    const backdrop = settingsModal.querySelector('.modal-backdrop');
    if (backdrop) {
        backdrop.addEventListener('click', closeSettings);
    }

    // Retry button
    document.getElementById('retry-btn').addEventListener('click', () => {
        if (currentUrl) {
            urlInput.value = currentUrl;
            handleFetch();
        }
    });

    // Tab switching
    document.querySelectorAll('.tab').forEach(tab => {
        tab.addEventListener('click', () => switchTab(tab.dataset.tab));
    });

    // Collapsed menu
    const collapsedMenuBtn = document.getElementById('collapsed-menu-btn');
    const navDropdown = document.getElementById('nav-dropdown');

    if (collapsedMenuBtn && navDropdown) {
        collapsedMenuBtn.addEventListener('click', (e) => {
            e.stopPropagation();
            navDropdown.classList.toggle('open');
        });

        // Close dropdown when clicking outside
        document.addEventListener('click', () => {
            navDropdown.classList.remove('open');
        });

        // Dropdown item actions
        navDropdown.querySelectorAll('.nav-dropdown-item').forEach(item => {
            item.addEventListener('click', () => {
                const action = item.dataset.action;
                navDropdown.classList.remove('open');

                switch (action) {
                    case 'recent':
                        switchTab('recent');
                        break;
                    case 'favorites':
                        switchTab('favorites');
                        break;
                    case 'favorite':
                        toggleFavorite();
                        break;
                    case 'copy':
                        copyAsMarkdown();
                        break;
                    case 'save':
                        saveAsMarkdown();
                        break;
                }
            });
        });
    }

    // Track scroll position to toggle hero-visible class
    articleContainer.addEventListener('scroll', () => {
        const heroEl = document.getElementById('article-hero');
        if (!heroEl || heroEl.classList.contains('no-image')) return;

        const heroHeight = heroEl.offsetHeight;
        const scrollTop = articleContainer.scrollTop;

        // Add class when scrolled past the hero image (account for titlebar + nav-bar + action buttons)
        if (scrollTop >= heroHeight - 160) {
            articleContainer.classList.add('scrolled-past-hero');
        } else {
            articleContainer.classList.remove('scrolled-past-hero');
        }
    });
}

function setupWindowDrag() {
    const header = document.querySelector('#app > header');
    if (!header) return;

    header.addEventListener('mousedown', async (e) => {
        const target = e.target;
        const isInteractive = target.closest('input, button, select, a, .search-field, .toolbar-group');

        if (!isInteractive && e.buttons === 1) {
            e.preventDefault();
            try {
                await getCurrentWindow().startDragging();
            } catch (err) {
                console.error('Failed to start window drag:', err);
            }
        }
    });
}

function setupSwipeGestures() {
    // Handle mouse back/forward buttons (side buttons on mice)
    window.addEventListener('mouseup', (e) => {
        if (e.button === 3) { // Back button
            e.preventDefault();
            goBack();
        } else if (e.button === 4) { // Forward button
            e.preventDefault();
            goForward();
        }
    });

    // Use browser History API so native swipe gestures work
    // When we navigate, push state; when user swipes, popstate fires
    window.addEventListener('popstate', (e) => {
        if (e.state) {
            if (e.state.type === 'article' && e.state.url) {
                // Load article from history without pushing new state
                loadArticleFromHistoryState(e.state);
            } else if (e.state.type === 'home') {
                showStartPageWithoutPush();
            }
        } else {
            // No state = initial page = home
            showStartPageWithoutPush();
        }
    });

    // Push initial state
    if (!window.history.state) {
        window.history.replaceState({ type: 'home' }, '', window.location.href);
    }
}

function setupKeyboardShortcuts() {
    document.addEventListener('keydown', (e) => {
        // Cmd+, for settings
        if (e.metaKey && e.key === ',') {
            e.preventDefault();
            if (settingsModal.classList.contains('hidden')) {
                openSettings();
            } else {
                closeSettings();
            }
        }

        // Escape to close modal, blur inputs, clear selection, or go to start page
        if (e.key === 'Escape') {
            if (!settingsModal.classList.contains('hidden')) {
                closeSettings();
            } else if (document.activeElement === urlInput || document.activeElement === searchInput) {
                // Blur the input field
                document.activeElement.blur();
            } else if (selectedListIndex >= 0) {
                // Clear list selection
                clearListSelection();
            } else if (!articleContainer.classList.contains('hidden')) {
                showStartPage();
            }
        }

        // Cmd+L to focus URL input
        if (e.metaKey && e.key === 'l') {
            e.preventDefault();
            urlInput.focus();
            urlInput.select();
        }

        // Cmd+K to open search (only on start page)
        if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
            e.preventDefault();
            if (!startPage.classList.contains('hidden')) {
                if (isSearchMode) {
                    closeSearch();
                } else {
                    openSearch();
                }
            }
        }

        // Cmd+R to refresh current article
        if (e.metaKey && e.key === 'r' && currentUrl) {
            e.preventDefault();
            handleFetch(true); // Force refresh
        }

        // Cmd+[ and Cmd+] for navigation
        if (e.metaKey && e.key === '[') {
            e.preventDefault();
            goBack();
        }
        if (e.metaKey && e.key === ']') {
            e.preventDefault();
            goForward();
        }

        // Cmd+D to toggle favorite
        if (e.metaKey && e.key === 'd' && currentUrl) {
            e.preventDefault();
            toggleFavorite();
        }

        // Cmd+Shift+C to copy as Markdown
        if (e.metaKey && e.shiftKey && e.key === 'c' && currentArticle) {
            e.preventDefault();
            copyAsMarkdown();
        }

        // Cmd+S to save as Markdown
        if (e.metaKey && e.key === 's' && currentArticle) {
            e.preventDefault();
            saveAsMarkdown();
        }

        // Arrow key navigation in article list
        // Only when: on start page, not in an input field (except search input)
        const isInInput = document.activeElement?.tagName === 'INPUT' ||
                          document.activeElement?.tagName === 'TEXTAREA';
        const isInSearchInput = document.activeElement === searchInput;
        const canNavigateList = !startPage.classList.contains('hidden') &&
                                (!isInInput || isInSearchInput) &&
                                !e.metaKey && !e.ctrlKey && !e.altKey;

        if (canNavigateList) {
            if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
                e.preventDefault();
                navigateList(e.key === 'ArrowDown' ? 1 : -1);
            }
            if (e.key === 'Enter' && selectedListIndex >= 0) {
                // Only if not in URL input (which has its own Enter handler)
                if (document.activeElement !== urlInput) {
                    e.preventDefault();
                    openSelectedArticle();
                }
            }
        }
    });
}

// Arrow key navigation for article list
function navigateList(direction) {
    const activeTab = document.querySelector('.tab.active');
    const listId = activeTab?.dataset.tab === 'favorites' ? 'favorites-list' : 'recent-list';
    const list = document.getElementById(listId);
    const items = list.querySelectorAll('.article-list-item');

    if (items.length === 0) return;

    // Remove selection from current item
    if (selectedListIndex >= 0 && selectedListIndex < items.length) {
        items[selectedListIndex].classList.remove('selected');
    }

    // Calculate new index
    if (selectedListIndex === -1) {
        // No selection yet, start at first or last depending on direction
        selectedListIndex = direction === 1 ? 0 : items.length - 1;
    } else {
        selectedListIndex += direction;
        // Wrap around
        if (selectedListIndex < 0) selectedListIndex = items.length - 1;
        if (selectedListIndex >= items.length) selectedListIndex = 0;
    }

    // Add selection to new item
    items[selectedListIndex].classList.add('selected');

    // Scroll into view
    items[selectedListIndex].scrollIntoView({ block: 'nearest', behavior: 'smooth' });
}

function openSelectedArticle() {
    const activeTab = document.querySelector('.tab.active');
    const listId = activeTab?.dataset.tab === 'favorites' ? 'favorites-list' : 'recent-list';
    const list = document.getElementById(listId);
    const items = list.querySelectorAll('.article-list-item');

    if (selectedListIndex >= 0 && selectedListIndex < items.length) {
        const url = items[selectedListIndex].dataset.url;
        if (url) {
            urlInput.value = url;
            handleFetch();
        }
    }
}

function clearListSelection() {
    const lists = ['recent-list', 'favorites-list'];
    lists.forEach(listId => {
        const list = document.getElementById(listId);
        list.querySelectorAll('.article-list-item.selected').forEach(item => {
            item.classList.remove('selected');
        });
    });
    selectedListIndex = -1;
}

// Start page and tabs
async function loadStartPage() {
    await Promise.all([loadRecentArticles(), loadFavorites()]);
    showStartPage();
}

async function loadRecentArticles() {
    const list = document.getElementById('recent-list');
    const empty = document.getElementById('recent-empty');

    // Clear selection when reloading list
    selectedListIndex = -1;

    try {
        const articles = await invoke('get_history', { limit: 50 });
        if (articles.length === 0) {
            list.classList.add('hidden');
            empty.classList.remove('hidden');
        } else {
            const sorted = sortArticles(articles, currentSortOrder);
            list.innerHTML = sorted.map(article => renderArticleListItem(article)).join('');
            list.classList.remove('hidden');
            empty.classList.add('hidden');
            attachArticleListHandlers(list);
        }
    } catch (err) {
        console.error('Failed to load history:', err);
        list.classList.add('hidden');
        empty.classList.remove('hidden');
    }

    // Re-init icons for new content
    if (window.lucide) lucide.createIcons();
}

async function loadFavorites() {
    const list = document.getElementById('favorites-list');
    const empty = document.getElementById('favorites-empty');

    try {
        const articles = await invoke('get_favorites');
        if (articles.length === 0) {
            list.classList.add('hidden');
            empty.classList.remove('hidden');
        } else {
            const sorted = sortArticles(articles, currentSortOrder);
            list.innerHTML = sorted.map(article => renderArticleListItem(article)).join('');
            list.classList.remove('hidden');
            empty.classList.add('hidden');
            attachArticleListHandlers(list);
        }
    } catch (err) {
        console.error('Failed to load favorites:', err);
        list.classList.add('hidden');
        empty.classList.remove('hidden');
    }

    if (window.lucide) lucide.createIcons();
}

function renderArticleListItem(article) {
    const date = new Date(article.last_read_at);
    const timeAgo = formatTimeAgo(date);

    const thumbnail = article.header_image_url
        ? `<div class="article-list-item-thumbnail">
               <img src="${escapeHtml(article.header_image_url)}" alt="" loading="lazy" referrerpolicy="no-referrer">
           </div>`
        : '';

    return `
        <div class="article-list-item" data-url="${escapeHtml(article.url)}">
            ${thumbnail}
            <div class="article-list-item-content">
                <div class="article-list-item-title">${escapeHtml(article.title)}</div>
                <div class="article-list-item-meta">
                    <span>${escapeHtml(article.author)}</span>
                    <span>•</span>
                    <span>${timeAgo}</span>
                </div>
            </div>
            <div class="article-list-item-actions">
                <button class="action-btn copy-url-btn" data-url="${escapeHtml(article.url)}" title="Copy URL">
                    <i data-lucide="link" width="14" height="14"></i>
                </button>
                <button class="action-btn favorite-item-btn ${article.is_favorite ? 'favorited' : ''}"
                        data-url="${escapeHtml(article.url)}" title="${article.is_favorite ? 'Remove from favorites' : 'Add to favorites'}">
                    <i data-lucide="heart" width="14" height="14"></i>
                </button>
                <button class="action-btn delete-item-btn" data-url="${escapeHtml(article.url)}" title="Remove from history">
                    <i data-lucide="trash-2" width="14" height="14"></i>
                </button>
            </div>
        </div>
    `;
}

function attachArticleListHandlers(list) {
    // Click to open article
    list.querySelectorAll('.article-list-item').forEach(item => {
        item.addEventListener('click', (e) => {
            if (e.target.closest('.article-list-item-actions')) return;
            const url = item.dataset.url;
            urlInput.value = url;
            handleFetch();
        });
    });

    // Copy URL buttons
    list.querySelectorAll('.copy-url-btn').forEach(btn => {
        btn.addEventListener('click', async (e) => {
            e.stopPropagation();
            const url = btn.dataset.url;
            try {
                await writeText(url);
                // Visual feedback
                btn.classList.add('copy-success');
                const icon = btn.querySelector('i');
                if (icon) {
                    icon.setAttribute('data-lucide', 'check');
                    lucide.createIcons();
                }
                setTimeout(() => {
                    btn.classList.remove('copy-success');
                    if (icon) {
                        icon.setAttribute('data-lucide', 'link');
                        lucide.createIcons();
                    }
                }, 1500);
            } catch (err) {
                console.error('Failed to copy URL:', err);
            }
        });
    });

    // Favorite buttons
    list.querySelectorAll('.favorite-item-btn').forEach(btn => {
        btn.addEventListener('click', async (e) => {
            e.stopPropagation();
            const url = btn.dataset.url;
            try {
                const isFavorite = await invoke('toggle_favorite', { url });
                btn.classList.toggle('favorited', isFavorite);
                btn.title = isFavorite ? 'Remove from favorites' : 'Add to favorites';

                // Confetti burst when adding to favorites
                if (isFavorite && window.confetti) {
                    const rect = btn.getBoundingClientRect();
                    const x = (rect.left + rect.width / 2) / window.innerWidth;
                    const y = (rect.top + rect.height / 2) / window.innerHeight;

                    confetti({
                        particleCount: 30,
                        spread: 100,
                        origin: { x, y },
                        colors: ['#E54D2E', '#E5A84B', '#FF6B6B', '#FFE66D'],
                        scalar: 0.6,
                        gravity: 0.5,
                        startVelocity: 15,
                        angle: 90,
                        ticks: 250
                    });
                }

                // No list reload - just update UI state
                // Lists will refresh on next tab switch or page load
            } catch (err) {
                console.error('Failed to toggle favorite:', err);
            }
        });
    });

    // Delete buttons - with confirmation
    list.querySelectorAll('.delete-item-btn').forEach(btn => {
        btn.addEventListener('click', async (e) => {
            e.stopPropagation();
            const url = btn.dataset.url;
            const item = btn.closest('.article-list-item');
            const title = item?.querySelector('.article-list-item-title')?.textContent || 'this article';

            // Show confirmation
            const confirmed = await showConfirmDialog(
                'Remove Article',
                `Are you sure you want to remove "${title}" from your history?`
            );

            if (confirmed) {
                try {
                    await invoke('delete_from_history', { url });
                    await loadRecentArticles();
                    await loadFavorites();
                } catch (err) {
                    console.error('Failed to delete:', err);
                }
            }
        });
    });
}

function switchTab(tabName) {
    // If viewing an article, navigate to start page first
    if (!articleContainer.classList.contains('hidden')) {
        hideAll();
        startPage.classList.remove('hidden');
        currentUrl = '';
        currentArticle = null;
        urlInput.value = '';
        loadRecentArticles();
        loadFavorites();
    }

    // Clear list selection when switching tabs
    clearListSelection();

    // Update tab buttons
    document.querySelectorAll('.tab').forEach(tab => {
        tab.classList.toggle('active', tab.dataset.tab === tabName);
    });

    // Update tab content
    document.querySelectorAll('.tab-content').forEach(content => {
        content.classList.toggle('active', content.id === `${tabName}-tab`);
        content.classList.toggle('hidden', content.id !== `${tabName}-tab`);
    });
}

// Navigation
function goBack() {
    // Use browser history for native gesture support
    window.history.back();
}

function goForward() {
    // Use browser history for native gesture support
    window.history.forward();
}

function loadArticleFromHistory(entry) {
    currentUrl = entry.url;
    currentArticle = entry.article;
    urlInput.value = entry.url;
    historyIndex = history.findIndex(h => h.url === entry.url);
    renderArticle(entry.article);
    updateFavoriteButton();
    updateNavButtons();
}

// Called from popstate handler - doesn't push new state
function loadArticleFromHistoryState(state) {
    const entry = history.find(h => h.url === state.url);
    if (entry) {
        currentUrl = entry.url;
        currentArticle = entry.article;
        urlInput.value = entry.url;
        historyIndex = history.findIndex(h => h.url === entry.url);
        renderArticle(entry.article);
        updateFavoriteButton();
        updateNavButtons();
    }
}

// Called from popstate handler - doesn't push new state
function showStartPageWithoutPush() {
    hideAll();
    startPage.classList.remove('hidden');
    currentUrl = '';
    currentArticle = null;
    urlInput.value = '';
    historyIndex = -1;
    updateNavButtons();
    loadRecentArticles();
    loadFavorites();
}

function addToHistory(url, article) {
    // Remove any forward history
    if (historyIndex < history.length - 1) {
        history = history.slice(0, historyIndex + 1);
    }
    history.push({ url, article });
    historyIndex = history.length - 1;

    // Push to browser history for native gesture support
    window.history.pushState({ type: 'article', url }, '', window.location.href);

    updateNavButtons();
}

function updateNavButtons() {
    // Back is available if we have any history
    backBtn.disabled = historyIndex < 0;
    // Forward is available if we're not at the end
    forwardBtn.disabled = historyIndex >= history.length - 1;
}

// Fetch article
async function handleFetch(forceRefresh = false) {
    const url = urlInput.value.trim();

    if (!url) {
        urlInput.focus();
        return;
    }

    if (!isValidMediumUrl(url)) {
        showError('Please enter a valid Medium article URL');
        return;
    }

    currentUrl = url;
    showLoading();

    try {
        const article = await invoke('fetch_article', { url, forceRefresh });
        currentArticle = article;
        addToHistory(url, article);
        renderArticle(article);
        updateFavoriteButton();
    } catch (err) {
        showError(err.toString());
    }
}

function renderArticle(article) {
    document.getElementById('article-title').textContent = article.title;

    // Render author with link if available
    const authorEl = document.getElementById('article-author');
    if (article.author_url) {
        authorEl.innerHTML = `By <a href="${escapeHtml(article.author_url)}" target="_blank" rel="noopener" class="author-link">${escapeHtml(article.author)}</a>`;
    } else {
        authorEl.textContent = `By ${article.author}`;
    }

    // Set hero background image
    const heroEl = document.getElementById('article-hero');
    if (heroEl) {
        if (article.header_image_url) {
            heroEl.style.backgroundImage = `url('${escapeHtml(article.header_image_url)}')`;
            heroEl.classList.remove('no-image');
        } else {
            heroEl.style.backgroundImage = 'none';
            heroEl.classList.add('no-image');
        }
    }

    document.getElementById('article-content').innerHTML = article.content_html;
    document.getElementById('original-link').href = article.url || article.original_url;

    try {
        const hostname = new URL(article.fetched_from).hostname;
        document.getElementById('fetched-from').textContent = `via ${hostname}`;
    } catch {
        document.getElementById('fetched-from').textContent = '';
    }

    hideAll();
    hideLoading();
    articleContainer.classList.remove('hidden');
    articleContainer.classList.remove('scrolled-past-hero');
    articleContainer.scrollTop = 0;

    // Blur input so it doesn't stay focused after article loads
    urlInput.blur();

    // Re-init icons
    if (window.lucide) lucide.createIcons();
}

// Favorites
async function toggleFavorite() {
    if (!currentUrl) return;

    try {
        const isFavorite = await invoke('toggle_favorite', { url: currentUrl });
        if (currentArticle) {
            currentArticle.is_favorite = isFavorite;
        }
        updateFavoriteButton();

        // Confetti burst when adding to favorites
        if (isFavorite && window.confetti) {
            const rect = favoriteBtn.getBoundingClientRect();
            const x = (rect.left + rect.width / 2) / window.innerWidth;
            const y = (rect.top + rect.height / 2) / window.innerHeight;

            confetti({
                particleCount: 30,
                spread: 100,
                origin: { x, y },
                colors: ['#E54D2E', '#E5A84B', '#FF6B6B', '#FFE66D'],
                scalar: 0.6,
                gravity: 0.5,
                startVelocity: 15,
                angle: 90,
                ticks: 250
            });
        }
    } catch (err) {
        console.error('Failed to toggle favorite:', err);
    }
}

function updateFavoriteButton() {
    if (currentArticle) {
        favoriteBtn.classList.toggle('favorited', currentArticle.is_favorite);
        favoriteBtn.title = currentArticle.is_favorite ? 'Remove from Favorites' : 'Add to Favorites';

        // Update the icon (filled vs outline)
        const iconName = currentArticle.is_favorite ? 'heart' : 'heart';
        favoriteBtn.innerHTML = `<i data-lucide="${iconName}" width="16" height="16"></i>`;
        if (window.lucide) lucide.createIcons();

        // Update dropdown favorite button too
        const dropdownFavoriteBtn = document.getElementById('dropdown-favorite-btn');
        if (dropdownFavoriteBtn) {
            dropdownFavoriteBtn.classList.toggle('favorited', currentArticle.is_favorite);
            dropdownFavoriteBtn.innerHTML = `<i data-lucide="heart" width="16" height="16"></i> ${currentArticle.is_favorite ? 'Remove from Favorites' : 'Add to Favorites'}`;
            if (window.lucide) lucide.createIcons();
        }
    }
}

// View states
function showLoading() {
    const overlay = document.getElementById('loading-overlay');
    // If viewing an article, use overlay instead of hiding everything
    if (!articleContainer.classList.contains('hidden')) {
        overlay.classList.add('visible');
    } else {
        hideAll();
        loading.classList.remove('hidden');
    }
}

function hideLoading() {
    const overlay = document.getElementById('loading-overlay');
    overlay.classList.remove('visible');
    loading.classList.add('hidden');
}

function showError(message) {
    hideAll();
    errorMessage.textContent = message;
    error.classList.remove('hidden');
}

function showStartPage() {
    hideAll();
    startPage.classList.remove('hidden');
    currentUrl = '';
    currentArticle = null;
    urlInput.value = '';
    historyIndex = -1;

    // Clear list selection
    clearListSelection();

    // Push to browser history for native gesture support
    window.history.pushState({ type: 'home' }, '', window.location.href);

    updateNavButtons();
    loadRecentArticles();
    loadFavorites();
}

function hideAll() {
    startPage.classList.add('hidden');
    loading.classList.add('hidden');
    error.classList.add('hidden');
    articleContainer.classList.add('hidden');
}

// Theme and appearance
function applyTheme(theme) {
    let effectiveTheme = theme;
    if (theme === 'system') {
        effectiveTheme = window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
    }
    document.body.dataset.theme = effectiveTheme;
}

function applyFontSize(size) {
    document.documentElement.style.setProperty('--article-font-size', `${size}px`);
}

// Settings
function openSettings() {
    document.getElementById('theme-select').value = config.theme;
    document.getElementById('font-size').value = config.font_size;
    document.getElementById('font-size-value').textContent = `${config.font_size}px`;
    renderEndpointsList();
    settingsModal.classList.remove('hidden');
}

function closeSettings() {
    settingsModal.classList.add('hidden');
}

async function saveSettingsSilent(changes) {
    const newConfig = { ...config, ...changes };

    try {
        await invoke('save_config', { config: newConfig });
        config = newConfig;
    } catch (err) {
        console.error('Failed to save settings:', err);
    }
}

function renderEndpointsList() {
    const list = document.getElementById('endpoints-list');
    list.innerHTML = config.endpoints.map((ep, i) => `
        <div class="endpoint-item">
            <span class="endpoint-url">${ep}</span>
            <span class="endpoint-status" data-index="${i}">—</span>
        </div>
    `).join('');
}

async function checkEndpoints() {
    const btn = document.getElementById('check-endpoints-btn');
    btn.disabled = true;
    btn.textContent = 'Checking...';

    document.querySelectorAll('.endpoint-status').forEach(el => {
        el.textContent = '...';
        el.className = 'endpoint-status checking';
    });

    try {
        const results = await invoke('check_endpoints');
        results.forEach(([endpoint, alive], i) => {
            const el = document.querySelector(`[data-index="${i}"]`);
            if (el) {
                el.textContent = alive ? '✓' : '✗';
                el.className = `endpoint-status ${alive ? 'alive' : 'dead'}`;
            }
        });
    } catch (err) {
        console.error('Failed to check endpoints:', err);
        document.querySelectorAll('.endpoint-status').forEach(el => {
            el.textContent = '?';
            el.className = 'endpoint-status unknown';
        });
    } finally {
        btn.disabled = false;
        btn.textContent = 'Check Status';
    }
}

// Export database file
async function exportDatabase() {
    const btn = document.getElementById('export-db-btn');
    btn.disabled = true;
    btn.textContent = 'Exporting...';

    try {
        const filePath = await save({
            defaultPath: 'wallflower-backup.db',
            filters: [{
                name: 'SQLite Database',
                extensions: ['db']
            }]
        });

        if (filePath) {
            await invoke('export_database', { path: filePath });
            btn.textContent = 'Exported!';
            setTimeout(() => {
                btn.textContent = 'Export Database';
            }, 2000);
        } else {
            btn.textContent = 'Export Database';
        }
    } catch (err) {
        console.error('Failed to export database:', err);
        btn.textContent = 'Export Failed';
        setTimeout(() => {
            btn.textContent = 'Export Database';
        }, 2000);
    } finally {
        btn.disabled = false;
    }
}

// Export articles as Markdown files
async function exportAsMarkdown() {
    const btn = document.getElementById('export-md-btn');
    btn.disabled = true;
    btn.textContent = 'Exporting...';

    try {
        const folderPath = await open({
            directory: true,
            title: 'Select export folder'
        });

        if (folderPath) {
            const count = await invoke('export_as_markdown', { path: folderPath });
            btn.textContent = `Exported ${count} articles!`;
            setTimeout(() => {
                btn.textContent = 'Export as Markdown';
            }, 2000);
        } else {
            btn.textContent = 'Export as Markdown';
        }
    } catch (err) {
        console.error('Failed to export as Markdown:', err);
        btn.textContent = 'Export Failed';
        setTimeout(() => {
            btn.textContent = 'Export as Markdown';
        }, 2000);
    } finally {
        btn.disabled = false;
    }
}

// URL validation
function isValidMediumUrl(url) {
    if (!url) return false;

    try {
        const parsed = new URL(url);
        const host = parsed.hostname.toLowerCase();

        if (host === 'medium.com' || host.endsWith('.medium.com')) {
            return true;
        }

        const mediumDomains = [
            'towardsdatascience.com', 'betterprogramming.pub', 'levelup.gitconnected.com',
            'javascript.plainenglish.io', 'python.plainenglish.io', 'blog.devgenius.io',
            'uxdesign.cc', 'bootcamp.uxdesign.cc', 'betterhumans.pub', 'eand.co',
            'entrepreneurshandbook.co', 'writingcooperative.com', 'psiloveyou.xyz',
            'hackernoon.com', 'codeburst.io', 'itnext.io'
        ];

        if (mediumDomains.some(d => host === d || host.endsWith('.' + d))) {
            return true;
        }

        const mediumIdPattern = /-[a-f0-9]{10,12}$/;
        if (mediumIdPattern.test(parsed.pathname)) {
            return true;
        }

        return false;
    } catch {
        return false;
    }
}

// ===========================================
// Sort Functions
// ===========================================

function setSortOrder(sortOrder) {
    currentSortOrder = sortOrder;

    // Update button icon
    const iconName = sortIcons[sortOrder];
    sortBtn.innerHTML = `<i data-lucide="${iconName}" width="16" height="16"></i>`;
    if (window.lucide) lucide.createIcons();

    // Update active state in dropdown
    sortDropdown.querySelectorAll('.sort-dropdown-item').forEach(item => {
        item.classList.toggle('active', item.dataset.sort === sortOrder);
    });

    // Reload lists with new sort order
    loadRecentArticles();
    loadFavorites();
}

function sortArticles(articles, sortOrder) {
    const sorted = [...articles];

    switch (sortOrder) {
        case 'title':
            sorted.sort((a, b) => a.title.localeCompare(b.title));
            break;
        case 'author':
            sorted.sort((a, b) => a.author.localeCompare(b.author));
            break;
        case 'added':
            // cached_at is when first added (approximation since we don't have a separate field)
            sorted.sort((a, b) => new Date(b.last_read_at) - new Date(a.last_read_at));
            break;
        case 'recent':
        default:
            // Already sorted by last_read_at from backend
            break;
    }

    return sorted;
}

// ===========================================
// Search Functions
// ===========================================

function openSearch() {
    isSearchMode = true;
    navBar.classList.add('search-active');
    searchOverlay.classList.remove('hidden');
    searchInput.focus();

    // Re-init icons
    if (window.lucide) lucide.createIcons();
}

function closeSearch() {
    isSearchMode = false;
    navBar.classList.remove('search-active');
    searchOverlay.classList.add('hidden');
    searchClearBtn.classList.add('hidden');

    // Clear list selection
    clearListSelection();

    // Clear any pending debounce
    if (searchDebounceTimer) {
        clearTimeout(searchDebounceTimer);
        searchDebounceTimer = null;
    }

    // Only reload if there was a search query (results are showing)
    const hadQuery = searchInput.value.trim().length > 0;
    searchInput.value = '';

    if (hadQuery) {
        // Reload current tab list to restore original view
        const activeTab = document.querySelector('.tab.active');
        if (activeTab) {
            if (activeTab.dataset.tab === 'recent') {
                loadRecentArticles();
            } else {
                loadFavorites();
            }
        }
    }
}

function clearSearch() {
    searchInput.value = '';
    searchClearBtn.classList.add('hidden');
    searchInput.focus();

    // Reload current tab list
    const activeTab = document.querySelector('.tab.active');
    if (activeTab) {
        if (activeTab.dataset.tab === 'recent') {
            loadRecentArticles();
        } else {
            loadFavorites();
        }
    }
}

function handleSearchInput() {
    const query = searchInput.value.trim();

    // Show/hide clear button
    if (query.length > 0) {
        searchClearBtn.classList.remove('hidden');
    } else {
        searchClearBtn.classList.add('hidden');
    }

    // Clear previous debounce timer
    if (searchDebounceTimer) {
        clearTimeout(searchDebounceTimer);
    }

    // If empty, show normal list
    if (!query) {
        const activeTab = document.querySelector('.tab.active');
        if (activeTab) {
            if (activeTab.dataset.tab === 'recent') {
                loadRecentArticles();
            } else {
                loadFavorites();
            }
        }
        return;
    }

    // Debounce search by 200ms
    searchDebounceTimer = setTimeout(async () => {
        await performSearch(query);
    }, 200);
}

async function performSearch(query) {
    const activeTab = document.querySelector('.tab.active');
    const listId = activeTab?.dataset.tab === 'favorites' ? 'favorites-list' : 'recent-list';
    const list = document.getElementById(listId);
    const emptyId = activeTab?.dataset.tab === 'favorites' ? 'favorites-empty' : 'recent-empty';
    const empty = document.getElementById(emptyId);

    try {
        const results = await invoke('search_articles', { query });

        if (results.length === 0) {
            list.classList.add('hidden');
            empty.classList.remove('hidden');
            empty.innerHTML = `
                <i data-lucide="search" width="32" height="32"></i>
                <p>No results for "<span class="query">${escapeHtml(query)}</span>"</p>
                <p class="hint">Try different keywords</p>
            `;
        } else {
            list.innerHTML = results.map(article => renderArticleListItem(article)).join('');
            list.classList.remove('hidden');
            empty.classList.add('hidden');
            attachArticleListHandlers(list);
        }
    } catch (err) {
        console.error('Search failed:', err);
        list.classList.add('hidden');
        empty.classList.remove('hidden');
        empty.innerHTML = `
            <i data-lucide="alert-triangle" width="32" height="32"></i>
            <p>Search failed</p>
            <p class="hint">${escapeHtml(err.toString())}</p>
        `;
    }

    // Re-init icons for new content
    if (window.lucide) lucide.createIcons();
}

// Utilities
function formatTimeAgo(date) {
    const now = new Date();
    const diffMs = now - date;
    const diffMins = Math.floor(diffMs / 60000);
    const diffHours = Math.floor(diffMs / 3600000);
    const diffDays = Math.floor(diffMs / 86400000);

    if (diffMins < 1) return 'Just now';
    if (diffMins < 60) return `${diffMins}m ago`;
    if (diffHours < 24) return `${diffHours}h ago`;
    if (diffDays < 7) return `${diffDays}d ago`;

    return date.toLocaleDateString();
}

function escapeHtml(str) {
    const div = document.createElement('div');
    div.textContent = str;
    return div.innerHTML;
}

// Confirmation dialog
function showConfirmDialog(title, message) {
    return new Promise((resolve) => {
        // Create modal
        const modal = document.createElement('div');
        modal.className = 'confirm-modal';
        modal.innerHTML = `
            <div class="confirm-backdrop"></div>
            <div class="confirm-content">
                <h3>${escapeHtml(title)}</h3>
                <p>${escapeHtml(message)}</p>
                <div class="confirm-actions">
                    <button class="btn confirm-cancel">Cancel</button>
                    <button class="btn btn-danger confirm-ok">Remove</button>
                </div>
            </div>
        `;

        document.body.appendChild(modal);

        // Handle clicks
        const cleanup = (result) => {
            modal.remove();
            resolve(result);
        };

        modal.querySelector('.confirm-backdrop').addEventListener('click', () => cleanup(false));
        modal.querySelector('.confirm-cancel').addEventListener('click', () => cleanup(false));
        modal.querySelector('.confirm-ok').addEventListener('click', () => cleanup(true));

        // Handle escape key
        const handleKey = (e) => {
            if (e.key === 'Escape') {
                cleanup(false);
                document.removeEventListener('keydown', handleKey);
            }
        };
        document.addEventListener('keydown', handleKey);

        // Focus the cancel button
        setTimeout(() => modal.querySelector('.confirm-cancel').focus(), 10);
    });
}

// System theme changes
window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
    if (config?.theme === 'system') {
        applyTheme('system');
    }
});

// ===========================================
// Markdown Export Functions
// ===========================================

// Convert HTML to Markdown
function htmlToMarkdown(html) {
    const tempDiv = document.createElement('div');
    tempDiv.innerHTML = html;

    function processNode(node) {
        if (node.nodeType === Node.TEXT_NODE) {
            return node.textContent;
        }

        if (node.nodeType !== Node.ELEMENT_NODE) {
            return '';
        }

        const tag = node.tagName.toLowerCase();
        const children = Array.from(node.childNodes).map(processNode).join('');

        switch (tag) {
            case 'h1':
                return `# ${children.trim()}\n\n`;
            case 'h2':
                return `## ${children.trim()}\n\n`;
            case 'h3':
                return `### ${children.trim()}\n\n`;
            case 'h4':
                return `#### ${children.trim()}\n\n`;
            case 'h5':
                return `##### ${children.trim()}\n\n`;
            case 'h6':
                return `###### ${children.trim()}\n\n`;
            case 'p':
                return `${children.trim()}\n\n`;
            case 'br':
                return '\n';
            case 'hr':
                return '---\n\n';
            case 'strong':
            case 'b':
                return `**${children}**`;
            case 'em':
            case 'i':
                return `*${children}*`;
            case 'code':
                if (node.parentElement?.tagName.toLowerCase() === 'pre') {
                    return children;
                }
                return `\`${children}\``;
            case 'pre':
                const codeEl = node.querySelector('code');
                const codeContent = codeEl ? codeEl.textContent : children;
                const lang = codeEl?.className?.match(/language-(\w+)/)?.[1] || '';
                return `\`\`\`${lang}\n${codeContent.trim()}\n\`\`\`\n\n`;
            case 'blockquote':
                return children.trim().split('\n').map(line => `> ${line}`).join('\n') + '\n\n';
            case 'a':
                const href = node.getAttribute('href') || '';
                return `[${children}](${href})`;
            case 'img':
                const src = node.getAttribute('src') || '';
                const alt = node.getAttribute('alt') || '';
                return `![${alt}](${src})\n\n`;
            case 'ul':
                return processListItems(node, '-') + '\n';
            case 'ol':
                return processListItems(node, '1.') + '\n';
            case 'li':
                return children.trim();
            case 'figure':
                return children;
            case 'figcaption':
                return `*${children.trim()}*\n\n`;
            case 'table':
                return processTable(node) + '\n';
            case 'div':
            case 'span':
            case 'section':
            case 'article':
                return children;
            default:
                return children;
        }
    }

    function processListItems(listNode, marker) {
        const items = Array.from(listNode.children).filter(c => c.tagName.toLowerCase() === 'li');
        return items.map((li, i) => {
            const prefix = marker === '1.' ? `${i + 1}.` : marker;
            const content = processNode(li);
            return `${prefix} ${content}`;
        }).join('\n');
    }

    function processTable(tableNode) {
        const rows = Array.from(tableNode.querySelectorAll('tr'));
        if (rows.length === 0) return '';

        const result = [];

        rows.forEach((row, rowIndex) => {
            const cells = Array.from(row.querySelectorAll('th, td'));
            const cellContents = cells.map(cell => processNode(cell).trim().replace(/\|/g, '\\|'));
            result.push(`| ${cellContents.join(' | ')} |`);

            // Add separator after header row
            if (rowIndex === 0 && row.querySelector('th')) {
                result.push(`| ${cells.map(() => '---').join(' | ')} |`);
            }
        });

        return result.join('\n');
    }

    let markdown = processNode(tempDiv);

    // Clean up excessive newlines
    markdown = markdown.replace(/\n{3,}/g, '\n\n');
    markdown = markdown.trim();

    return markdown;
}

// Generate full Markdown document
function generateMarkdown() {
    if (!currentArticle) return null;

    const content = htmlToMarkdown(currentArticle.content_html);
    const sourceUrl = currentArticle.url || currentArticle.original_url;

    // Format author with link if available
    const authorLine = currentArticle.author_url
        ? `*By [${currentArticle.author}](${currentArticle.author_url})*`
        : `*By ${currentArticle.author}*`;

    // Include header image if available
    const headerImage = currentArticle.header_image_url
        ? `![Header image](${currentArticle.header_image_url})\n\n`
        : '';

    const markdown = `# ${currentArticle.title}

${authorLine}

---

${headerImage}${content}

---

*Original article: [${sourceUrl}](${sourceUrl})*
`;

    return markdown;
}

// Copy article as Markdown to clipboard
async function copyAsMarkdown() {
    if (!currentArticle) return;

    const markdown = generateMarkdown();
    if (!markdown) return;

    try {
        await writeText(markdown);

        // Visual feedback
        copyMarkdownBtn.classList.add('copy-success');
        const icon = copyMarkdownBtn.querySelector('i');
        if (icon) {
            icon.setAttribute('data-lucide', 'check');
            lucide.createIcons();
        }

        setTimeout(() => {
            copyMarkdownBtn.classList.remove('copy-success');
            if (icon) {
                icon.setAttribute('data-lucide', 'clipboard-copy');
                lucide.createIcons();
            }
        }, 2000);
    } catch (err) {
        console.error('Failed to copy to clipboard:', err);
    }
}

// Save article as Markdown file
async function saveAsMarkdown() {
    if (!currentArticle) return;

    const markdown = generateMarkdown();
    if (!markdown) return;

    // Generate filename from title
    const safeTitle = currentArticle.title
        .replace(/[^a-z0-9]+/gi, '-')
        .replace(/^-|-$/g, '')
        .toLowerCase()
        .slice(0, 50);
    const defaultName = `${safeTitle}.md`;

    try {
        const filePath = await save({
            defaultPath: defaultName,
            filters: [{
                name: 'Markdown',
                extensions: ['md']
            }]
        });

        if (filePath) {
            // Use Tauri's fs plugin to write the file
            // Since we don't have fs plugin, we'll use a Rust command
            await invoke('save_markdown_file', { path: filePath, content: markdown });
        }
    } catch (err) {
        console.error('Failed to save file:', err);
    }
}
