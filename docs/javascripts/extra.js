/* Cello Framework - Custom JavaScript for Material for MkDocs */

// Initialize tablesort for all tables
document.addEventListener('DOMContentLoaded', function() {
  // Sort tables
  const tables = document.querySelectorAll('article table:not([class])');
  tables.forEach(table => {
    if (typeof Tablesort !== 'undefined') {
      new Tablesort(table);
    }
  });

  // Add copy button feedback
  const copyButtons = document.querySelectorAll('.md-clipboard');
  copyButtons.forEach(button => {
    button.addEventListener('click', () => {
      const originalText = button.getAttribute('data-clipboard-text');
      button.classList.add('copied');
      setTimeout(() => {
        button.classList.remove('copied');
      }, 2000);
    });
  });

  // Smooth scroll for anchor links
  document.querySelectorAll('a[href^="#"]').forEach(anchor => {
    anchor.addEventListener('click', function(e) {
      const href = this.getAttribute('href');
      if (href.length > 1) {
        const target = document.querySelector(href);
        if (target) {
          e.preventDefault();
          target.scrollIntoView({
            behavior: 'smooth',
            block: 'start'
          });
          // Update URL
          history.pushState(null, null, href);
        }
      }
    });
  });

  // Add external link indicator
  document.querySelectorAll('a[href^="http"]').forEach(link => {
    if (!link.hostname.includes('cello-framework')) {
      link.setAttribute('target', '_blank');
      link.setAttribute('rel', 'noopener noreferrer');
    }
  });

  // Initialize feedback system
  initFeedback();
});

// Feedback system
function initFeedback() {
  const feedbackContainer = document.querySelector('[data-md-feedback]');
  if (!feedbackContainer) return;

  feedbackContainer.querySelectorAll('[data-md-feedback-value]').forEach(button => {
    button.addEventListener('click', function() {
      const value = this.getAttribute('data-md-feedback-value');
      const page = window.location.pathname;

      // Send feedback (you can integrate with analytics)
      console.log('Feedback:', { page, value });

      // Show thank you message
      feedbackContainer.innerHTML = '<p>Thanks for your feedback!</p>';
    });
  });
}

// Performance monitoring (optional)
if (typeof performance !== 'undefined' && performance.mark) {
  performance.mark('cello-docs-loaded');
}

// Search enhancements
document.addEventListener('DOMContentLoaded', function() {
  const searchInput = document.querySelector('.md-search__input');
  if (searchInput) {
    // Add keyboard shortcut indicator
    const placeholder = searchInput.getAttribute('placeholder');
    if (navigator.platform.includes('Mac')) {
      searchInput.setAttribute('placeholder', placeholder + ' (⌘+K)');
    } else {
      searchInput.setAttribute('placeholder', placeholder + ' (Ctrl+K)');
    }
  }

  // Keyboard shortcut for search
  document.addEventListener('keydown', function(e) {
    if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
      e.preventDefault();
      const searchInput = document.querySelector('.md-search__input');
      if (searchInput) {
        searchInput.focus();
      }
    }
  });
});

// Version warning (for beta versions)
document.addEventListener('DOMContentLoaded', function() {
  const version = document.querySelector('.md-version__current');
  if (version && version.textContent.includes('0.')) {
    // Beta version - could add a banner
    console.info('Cello Framework beta version documentation');
  }
});
