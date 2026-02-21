/* Cello Framework - Custom JavaScript for Material for MkDocs */
/* Dark orange/black theme enhancements */

(function () {
  "use strict";

  // ──────────────────────────────────────────────
  // 1. Table Sorting
  // ──────────────────────────────────────────────
  function initTableSort() {
    var tables = document.querySelectorAll("article table:not([class])");
    tables.forEach(function (table) {
      if (typeof Tablesort !== "undefined") {
        new Tablesort(table);
      }
    });
  }

  // ──────────────────────────────────────────────
  // 2. Copy Button Feedback
  // ──────────────────────────────────────────────
  function initCopyFeedback() {
    var copyButtons = document.querySelectorAll(".md-clipboard");
    copyButtons.forEach(function (button) {
      button.addEventListener("click", function () {
        button.classList.add("copied");
        button.setAttribute("aria-label", "Copied!");
        setTimeout(function () {
          button.classList.remove("copied");
          button.setAttribute("aria-label", "Copy to clipboard");
        }, 2000);
      });
    });
  }

  // ──────────────────────────────────────────────
  // 3. Smooth Scroll for Anchor Links
  // ──────────────────────────────────────────────
  function initSmoothScroll() {
    document.querySelectorAll('a[href^="#"]').forEach(function (anchor) {
      anchor.addEventListener("click", function (e) {
        var href = this.getAttribute("href");
        if (href.length > 1) {
          var target = document.querySelector(href);
          if (target) {
            e.preventDefault();
            target.scrollIntoView({ behavior: "smooth", block: "start" });
            history.pushState(null, null, href);
          }
        }
      });
    });
  }

  // ──────────────────────────────────────────────
  // 4. External Link Handling
  // ──────────────────────────────────────────────
  function initExternalLinks() {
    document.querySelectorAll('a[href^="http"]').forEach(function (link) {
      if (!link.hostname.includes(window.location.hostname)) {
        link.setAttribute("target", "_blank");
        link.setAttribute("rel", "noopener noreferrer");
      }
    });
  }

  // ──────────────────────────────────────────────
  // 5. Animated Counters
  // ──────────────────────────────────────────────
  function initAnimatedCounters() {
    if (!("IntersectionObserver" in window)) return;

    // Match elements containing performance numbers like "150,000+" or "10x"
    var candidates = document.querySelectorAll(
      "h1, h2, h3, h4, p, li, td, th, strong, em, span"
    );
    var numberPattern = /^([\d,]+)\+?$/;
    var multiplierPattern = /^(\d+)x$/;

    candidates.forEach(function (el) {
      var text = el.textContent.trim();
      var match = text.match(numberPattern);
      var multMatch = text.match(multiplierPattern);

      if (match || multMatch) {
        el.setAttribute("data-counter-animated", "false");
      }
    });

    var counterElements = document.querySelectorAll(
      "[data-counter-animated='false']"
    );
    if (counterElements.length === 0) return;

    var counterObserver = new IntersectionObserver(
      function (entries) {
        entries.forEach(function (entry) {
          if (
            entry.isIntersecting &&
            entry.target.getAttribute("data-counter-animated") === "false"
          ) {
            entry.target.setAttribute("data-counter-animated", "true");
            animateCounter(entry.target);
          }
        });
      },
      { threshold: 0.3 }
    );

    counterElements.forEach(function (el) {
      counterObserver.observe(el);
    });
  }

  function animateCounter(el) {
    var text = el.textContent.trim();
    var hasPlusSuffix = text.endsWith("+");
    var hasXSuffix = text.endsWith("x");

    var rawNumber;
    if (hasXSuffix) {
      rawNumber = text.replace("x", "");
    } else {
      rawNumber = text.replace(/[+,]/g, "");
    }

    var targetValue = parseInt(rawNumber, 10);
    if (isNaN(targetValue) || targetValue === 0) return;

    var suffix = hasPlusSuffix ? "+" : hasXSuffix ? "x" : "";
    var useCommas = text.includes(",");
    var duration = 1200;
    var startTime = null;

    function formatNumber(num) {
      if (useCommas) {
        return num.toLocaleString("en-US");
      }
      return String(num);
    }

    function step(timestamp) {
      if (!startTime) startTime = timestamp;
      var progress = Math.min((timestamp - startTime) / duration, 1);
      // Ease-out cubic for smooth deceleration
      var eased = 1 - Math.pow(1 - progress, 3);
      var current = Math.floor(eased * targetValue);
      el.textContent = formatNumber(current) + suffix;
      if (progress < 1) {
        requestAnimationFrame(step);
      } else {
        el.textContent = formatNumber(targetValue) + suffix;
      }
    }

    el.textContent = formatNumber(0) + suffix;
    requestAnimationFrame(step);
  }

  // ──────────────────────────────────────────────
  // 6. Typing Effect for Hero Subtitle
  // ──────────────────────────────────────────────
  function initTypingEffect() {
    // Target the hero subtitle on the homepage
    var heroSubtitle = document.querySelector(
      ".md-typeset .hero-subtitle, .tx-hero__content p, .md-content h2:first-of-type"
    );

    // Only activate on the index/home page
    var isHomePage =
      window.location.pathname === "/" ||
      window.location.pathname.endsWith("/index.html") ||
      window.location.pathname.endsWith("/cello/");

    if (!heroSubtitle || !isHomePage) return;

    var phrases = [
      "Rust-powered performance",
      "Python simplicity",
      "Enterprise-grade security",
      "170,000+ requests/sec",
    ];

    var phraseIndex = 0;
    var charIndex = 0;
    var isDeleting = false;
    var typingSpeed = 60;
    var deletingSpeed = 35;
    var pauseBetween = 2000;

    // Store original text and replace with typing container
    var originalText = heroSubtitle.textContent;
    heroSubtitle.setAttribute("data-original-text", originalText);
    heroSubtitle.innerHTML = '<span class="typing-text"></span><span class="typing-cursor">|</span>';

    var typingTarget = heroSubtitle.querySelector(".typing-text");
    var cursor = heroSubtitle.querySelector(".typing-cursor");

    // Add cursor blink animation via inline style (CSS handles the rest)
    cursor.style.animation = "cello-blink 0.7s step-end infinite";

    function type() {
      var currentPhrase = phrases[phraseIndex];

      if (isDeleting) {
        charIndex--;
        typingTarget.textContent = currentPhrase.substring(0, charIndex);

        if (charIndex === 0) {
          isDeleting = false;
          phraseIndex = (phraseIndex + 1) % phrases.length;
          setTimeout(type, typingSpeed);
          return;
        }
        setTimeout(type, deletingSpeed);
      } else {
        charIndex++;
        typingTarget.textContent = currentPhrase.substring(0, charIndex);

        if (charIndex === currentPhrase.length) {
          isDeleting = true;
          setTimeout(type, pauseBetween);
          return;
        }
        setTimeout(type, typingSpeed);
      }
    }

    // Inject keyframe animation for cursor blink
    if (!document.getElementById("cello-typing-styles")) {
      var style = document.createElement("style");
      style.id = "cello-typing-styles";
      style.textContent =
        "@keyframes cello-blink { 0%, 100% { opacity: 1; } 50% { opacity: 0; } }" +
        ".typing-cursor { color: #FF6D00; font-weight: 300; margin-left: 2px; }";
      document.head.appendChild(style);
    }

    // Start after a short delay
    setTimeout(type, 500);
  }

  // ──────────────────────────────────────────────
  // 7. Scroll-Triggered Animations
  // ──────────────────────────────────────────────
  function initScrollAnimations() {
    if (!("IntersectionObserver" in window)) return;

    // Inject animation styles
    if (!document.getElementById("cello-scroll-styles")) {
      var style = document.createElement("style");
      style.id = "cello-scroll-styles";
      style.textContent =
        ".cello-animate { opacity: 0; transform: translateY(24px); transition: opacity 0.5s ease, transform 0.5s ease; }" +
        ".cello-animate.visible { opacity: 1; transform: translateY(0); }";
      document.head.appendChild(style);
    }

    // Target cards, sections, admonitions, code blocks, and content blocks
    var animatableSelectors = [
      ".md-typeset .admonition",
      ".md-typeset details",
      ".md-typeset .tabbed-set",
      ".md-typeset .highlight",
      ".md-typeset table",
      ".md-typeset blockquote",
      ".md-typeset h2",
      ".md-typeset .grid .card",
      ".md-typeset .md-typeset > .grid",
    ];

    var elements = document.querySelectorAll(animatableSelectors.join(", "));
    elements.forEach(function (el, index) {
      el.classList.add("cello-animate");
      // Stagger the transition delay for sequential appearance
      el.style.transitionDelay = Math.min(index % 4, 3) * 0.08 + "s";
    });

    var scrollObserver = new IntersectionObserver(
      function (entries) {
        entries.forEach(function (entry) {
          if (entry.isIntersecting) {
            entry.target.classList.add("visible");
            scrollObserver.unobserve(entry.target);
          }
        });
      },
      { threshold: 0.1, rootMargin: "0px 0px -40px 0px" }
    );

    document.querySelectorAll(".cello-animate").forEach(function (el) {
      scrollObserver.observe(el);
    });
  }

  // ──────────────────────────────────────────────
  // 8. Search Enhancement (Ctrl+K / Cmd+K)
  // ──────────────────────────────────────────────
  function initSearchEnhancement() {
    var searchInput = document.querySelector(".md-search__input");
    if (searchInput) {
      var placeholder = searchInput.getAttribute("placeholder") || "Search";
      var shortcut = navigator.platform.includes("Mac") ? " (\u2318+K)" : " (Ctrl+K)";
      searchInput.setAttribute("placeholder", placeholder + shortcut);
    }

    document.addEventListener("keydown", function (e) {
      if ((e.metaKey || e.ctrlKey) && e.key === "k") {
        e.preventDefault();
        var input = document.querySelector(".md-search__input");
        if (input) {
          input.focus();
        }
      }
    });
  }

  // ──────────────────────────────────────────────
  // 9. Theme Toggle Smooth Transition
  // ──────────────────────────────────────────────
  function initThemeTransition() {
    // Inject transition style for theme switches
    if (!document.getElementById("cello-theme-transition")) {
      var style = document.createElement("style");
      style.id = "cello-theme-transition";
      style.textContent =
        "body.cello-theme-transitioning, " +
        "body.cello-theme-transitioning * { " +
        "  transition: background-color 0.3s ease, color 0.3s ease, " +
        "  border-color 0.3s ease, box-shadow 0.3s ease !important; " +
        "}";
      document.head.appendChild(style);
    }

    // Observe the color scheme toggle buttons
    var toggles = document.querySelectorAll(
      "[data-md-color-scheme], .md-header__option label"
    );
    toggles.forEach(function (toggle) {
      toggle.addEventListener("click", function () {
        document.body.classList.add("cello-theme-transitioning");
        setTimeout(function () {
          document.body.classList.remove("cello-theme-transitioning");
        }, 400);
      });
    });

    // Also watch for changes via the palette toggle (Material for MkDocs)
    var paletteInputs = document.querySelectorAll('input[name="__palette"]');
    paletteInputs.forEach(function (input) {
      input.addEventListener("change", function () {
        document.body.classList.add("cello-theme-transitioning");
        setTimeout(function () {
          document.body.classList.remove("cello-theme-transitioning");
        }, 400);
      });
    });
  }

  // ──────────────────────────────────────────────
  // 10. Feedback System
  // ──────────────────────────────────────────────
  function initFeedback() {
    var feedbackContainer = document.querySelector("[data-md-feedback]");
    if (!feedbackContainer) return;

    feedbackContainer
      .querySelectorAll("[data-md-feedback-value]")
      .forEach(function (button) {
        button.addEventListener("click", function () {
          var value = this.getAttribute("data-md-feedback-value");
          var page = window.location.pathname;

          // Send feedback (integrate with analytics as needed)
          if (typeof gtag === "function") {
            gtag("event", "feedback", {
              event_category: "docs",
              event_label: page,
              value: value === "1" ? 1 : 0,
            });
          }

          console.log("Feedback:", { page: page, value: value });
          feedbackContainer.innerHTML =
            '<p style="margin:0;padding:8px 0;">Thanks for your feedback!</p>';
        });
      });
  }

  // ──────────────────────────────────────────────
  // 11. Performance Monitoring
  // ──────────────────────────────────────────────
  function initPerfMonitoring() {
    if (typeof performance !== "undefined" && performance.mark) {
      performance.mark("cello-docs-loaded");
    }
  }

  // ──────────────────────────────────────────────
  // Bootstrap on DOMContentLoaded
  // ──────────────────────────────────────────────
  document.addEventListener("DOMContentLoaded", function () {
    initTableSort();
    initCopyFeedback();
    initSmoothScroll();
    initExternalLinks();
    initAnimatedCounters();
    initTypingEffect();
    initScrollAnimations();
    initSearchEnhancement();
    initThemeTransition();
    initFeedback();
    initPerfMonitoring();
  });
})();
