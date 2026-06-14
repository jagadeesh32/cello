/* Cello Framework – Custom JS */
(function () {
  "use strict";

  // ── 1. Table Sorting ──────────────────────────────────────────────────────
  function initTableSort() {
    document.querySelectorAll("article table:not([class])").forEach(function (t) {
      if (t.classList.contains("tablesort-initialized")) return;
      if (typeof Tablesort !== "undefined") {
        new Tablesort(t);
        t.classList.add("tablesort-initialized");
      }
    });
  }

  // ── 2. Copy Button Feedback ───────────────────────────────────────────────
  function initCopyFeedback() {
    document.querySelectorAll(".md-clipboard").forEach(function (btn) {
      if (btn.classList.contains("copy-feedback-initialized")) return;
      btn.classList.add("copy-feedback-initialized");
      btn.addEventListener("click", function () {
        btn.classList.add("copied");
        btn.setAttribute("aria-label", "Copied!");
        setTimeout(function () {
          btn.classList.remove("copied");
          btn.setAttribute("aria-label", "Copy to clipboard");
        }, 2000);
      });
    });
  }

  // ── 3. Smooth Scroll ──────────────────────────────────────────────────────
  function initSmoothScroll() {
    document.querySelectorAll('a[href^="#"]').forEach(function (a) {
      if (a.classList.contains("smooth-scroll-initialized")) return;
      a.classList.add("smooth-scroll-initialized");
      a.addEventListener("click", function (e) {
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

  // ── 4. External Links ─────────────────────────────────────────────────────
  function initExternalLinks() {
    document.querySelectorAll('a[href^="http"]').forEach(function (link) {
      if (link.classList.contains("external-link-initialized")) return;
      link.classList.add("external-link-initialized");
      if (!link.hostname.includes(window.location.hostname)) {
        link.setAttribute("target", "_blank");
        link.setAttribute("rel", "noopener noreferrer");
      }
    });
  }

  // ── 5. Double Sidebar Layout ──────────────────────────────────────────────
  function initDoubleSidebar() {
    var isDesktop = window.matchMedia("(min-width: 76.2em)").matches;
    var mainInner = document.querySelector(".md-main__inner");
    var primarySidebar = document.querySelector(".md-sidebar--primary");
    
    // Inject icon badges into nav links
    injectIconBadges();

    // Clean up any existing sub-sidebar if not on desktop
    if (!isDesktop) {
      var existingSub = document.querySelector(".md-sidebar--sub");
      if (existingSub) existingSub.remove();
      document.body.classList.remove("no-sub-sidebar");
      return;
    }

    if (!mainInner || !primarySidebar) return;

    // Remove toggle button if it exists from older code
    var toggleBtn = document.getElementById("cello-nav-toggle");
    if (toggleBtn) toggleBtn.remove();



    // 1. Find the active top-level category item
    var navList = primarySidebar.querySelector(".md-nav > .md-nav__list");
    if (!navList) return;

    var activeItem = navList.querySelector(".md-nav__item--active");
    if (!activeItem) {
      // Find the first list item as a fallback
      activeItem = navList.querySelector(".md-nav__item");
    }

    // 2. Find the sub-navigation menu inside the active item
    var subNav = activeItem ? activeItem.querySelector(".md-nav[data-md-level='1']") : null;

    // Check if we are on the Home page (by tab title or pathname)
    var titleText = "";
    var linkTitleEl = activeItem ? (activeItem.querySelector("a.md-nav__link .md-ellipsis") || activeItem.querySelector("a.md-nav__link")) : null;
    if (linkTitleEl) {
      var clone = linkTitleEl.cloneNode(true);
      var badge = clone.querySelector(".nav-icon-badge");
      if (badge) badge.remove();
      titleText = clone.textContent.trim().toLowerCase();
    }
    
    var isHome = (titleText === "home" || titleText === "overview" || window.location.pathname === "/" || window.location.pathname.endsWith("/index.html") || window.location.pathname.endsWith("/index.htm"));

    if (isHome) {
      subNav = null; // Force disable sub-sidebar on Home page
    }

    var subSidebar = document.querySelector(".md-sidebar--sub");

    if (subNav) {
      // We have sub-pages, show sub-sidebar
      document.body.classList.remove("no-sub-sidebar");

      if (!subSidebar) {
        subSidebar = document.createElement("div");
        subSidebar.className = "md-sidebar md-sidebar--sub";
        mainInner.insertBefore(subSidebar, primarySidebar.nextSibling);
      }

      // Clear previous content
      subSidebar.innerHTML = "";

      // Clone subNav and append to subSidebar
      var clonedNav = subNav.cloneNode(true);
      clonedNav.style.display = "block"; // Make sure it's visible
      
      // Remove any md-nav__title inside the cloned nav to prevent duplicate headers
      clonedNav.querySelectorAll(".md-nav__title").forEach(function (el) {
        el.remove();
      });
      
      // Update IDs of toggles and labels to avoid duplicates and enable nested expand/collapse
      var toggles = clonedNav.querySelectorAll("input.md-toggle, input.md-nav__toggle");
      toggles.forEach(function (toggle) {
        var oldId = toggle.id;
        if (oldId) {
          var newId = oldId + "-sub";
          toggle.id = newId;
          
          // Find the corresponding label(s) in the cloned node
          var labels = clonedNav.querySelectorAll("label[for='" + oldId + "']");
          labels.forEach(function (lbl) {
            lbl.setAttribute("for", newId);
          });
        }
      });
      
      // Create a section title for the sub-sidebar
      var titleText = "";
      var sectionHref = "";
      var linkTitleEl = activeItem.querySelector("a.md-nav__link .md-ellipsis") || activeItem.querySelector("a.md-nav__link");
      if (linkTitleEl) {
        // Clone and strip badges to get clean text
        var clone = linkTitleEl.cloneNode(true);
        var badge = clone.querySelector(".nav-icon-badge");
        if (badge) badge.remove();
        titleText = clone.textContent.trim();
        
        var parentLink = activeItem.querySelector("a.md-nav__link");
        if (parentLink) {
          sectionHref = parentLink.getAttribute("href");
        }
      }

      if (titleText) {
        var sectionTitle = document.createElement("div");
        sectionTitle.className = "md-nav__title";
        sectionTitle.textContent = titleText;
        subSidebar.appendChild(sectionTitle);
      }

      // Check if we are already on the section overview page
      var isOverviewPage = false;
      if (sectionHref) {
        try {
          var currentUrl = new URL(window.location.href);
          var targetUrl = new URL(sectionHref, window.location.href);
          var currentPath = currentUrl.pathname.replace(/\/index\.html$/, "/").replace(/\/$/, "");
          var targetPath = targetUrl.pathname.replace(/\/index\.html$/, "/").replace(/\/$/, "");
          if (currentPath === targetPath) {
            isOverviewPage = true;
          }
        } catch (e) {
          // Ignore URL parsing errors
        }
      }

      if (sectionHref && titleText && !isOverviewPage) {
        var backBtn = document.createElement("a");
        backBtn.className = "sub-sidebar-back-btn";
        backBtn.setAttribute("href", sectionHref);
        backBtn.innerHTML = '<i class="fa-solid fa-arrow-left"></i> Back to Overview';
        subSidebar.appendChild(backBtn);
      }

      subSidebar.appendChild(clonedNav);
    } else {
      // No sub-pages (e.g. Home or Tags), hide sub-sidebar
      document.body.classList.add("no-sub-sidebar");
      if (subSidebar) {
        subSidebar.remove();
      }
    }

    // Always enable native tooltips on primary sidebar icons since it's permanently narrow
    var navLinks = document.querySelectorAll(".md-sidebar--primary a.md-nav__link, .md-sidebar--primary span.md-nav__link");
    navLinks.forEach(function (link) {
      if (link.classList.contains("md-nav__container")) return;
      if (link.tagName.toLowerCase() === "label") return;
      
      var text = link.getAttribute("data-title");
      if (text) {
        link.setAttribute("title", text);
      }
    });
  }

  var iconMap = {
    home: '<i class="fa-solid fa-house"></i>',
    getting_started: '<i class="fa-solid fa-rocket"></i>',
    features: '<i class="fa-solid fa-bolt"></i>',
    learn: '<i class="fa-solid fa-book-open"></i>',
    reference: '<i class="fa-solid fa-book"></i>',
    examples: '<i class="fa-solid fa-code"></i>',
    enterprise: '<i class="fa-solid fa-building"></i>',
    release: '<i class="fa-solid fa-clock-rotate-left"></i>',
    community: '<i class="fa-solid fa-users"></i>',
    tags: '<i class="fa-solid fa-tags"></i>',
    default: '<i class="fa-solid fa-file-lines"></i>'
  };

  function getIconForTitle(title) {
    var cleanTitle = title.toLowerCase().trim();
    
    if (cleanTitle === "home" || cleanTitle === "overview") return iconMap.home;
    if (cleanTitle.includes("getting started")) return iconMap.getting_started;
    if (cleanTitle === "features") return iconMap.features;
    if (cleanTitle === "learn") return iconMap.learn;
    if (cleanTitle === "reference") return iconMap.reference;
    if (cleanTitle === "examples") return iconMap.examples;
    if (cleanTitle === "enterprise") return iconMap.enterprise;
    if (cleanTitle.includes("release")) return iconMap.release;
    if (cleanTitle.includes("community")) return iconMap.community;
    if (cleanTitle.includes("tag")) return iconMap.tags;
    
    return iconMap.default;
  }

  function injectIconBadges() {
    var navLinks = document.querySelectorAll(
      ".md-sidebar--primary a.md-nav__link, .md-sidebar--primary span.md-nav__link"
    );
    navLinks.forEach(function (link) {
      if (link.classList.contains("md-nav__container")) return;
      if (link.tagName.toLowerCase() === "label") return;

      // Find nearest parent nav element
      var parentNav = link.closest("nav");
      if (!parentNav) return;

      // ONLY inject icons for top-level navigation items (level 0)
      var level = parentNav.getAttribute("data-md-level");
      if (level !== "0") {
        // This is a sub-page, remove any existing badges to keep it clean and tidy
        var existingBadge = link.querySelector(".nav-icon-badge");
        if (existingBadge) existingBadge.remove();
        return;
      }

      if (link.querySelector(".nav-icon-badge")) return;

      var ellipsisNode = link.querySelector(".md-ellipsis");
      var text = "";
      if (ellipsisNode) {
        var clone = ellipsisNode.cloneNode(true);
        var badg = clone.querySelector(".nav-icon-badge");
        if (badg) badg.remove();
        text = clone.textContent.trim();
      } else {
        var child = link.firstChild;
        while (child) {
          if (child.nodeType === 3) {
            text += child.textContent;
          }
          child = child.nextSibling;
        }
        text = text.trim();
      }

      if (!text) return;

      // Store tooltip
      if (!link.getAttribute("data-title")) {
        link.setAttribute("data-title", text);
      }

      var iconHTML = getIconForTitle(text);

      var badge = document.createElement("span");
      badge.className = "nav-icon-badge";
      badge.setAttribute("aria-hidden", "true");
      badge.innerHTML = iconHTML;
      link.insertBefore(badge, link.firstChild);
    });
  }

  // ── 6. Animated Counters ──────────────────────────────────────────────────
  function initAnimatedCounters() {
    if (!("IntersectionObserver" in window)) return;
    var numberPattern = /^([\d,]+)\+?$/;
    var multiplierPattern = /^(\d+)x$/;

    document.querySelectorAll("h1,h2,h3,h4,p,li,td,th,strong,em,span").forEach(function (el) {
      if (el.hasAttribute("data-counter-animated")) return;
      var text = el.textContent.trim();
      if (text.match(numberPattern) || text.match(multiplierPattern)) {
        el.setAttribute("data-counter-animated", "false");
      }
    });

    var els = document.querySelectorAll("[data-counter-animated='false']");
    if (!els.length) return;

    var obs = new IntersectionObserver(function (entries) {
      entries.forEach(function (entry) {
        if (entry.isIntersecting && entry.target.getAttribute("data-counter-animated") === "false") {
          entry.target.setAttribute("data-counter-animated", "true");
          animateCounter(entry.target);
        }
      });
    }, { threshold: 0.3 });

    els.forEach(function (el) { obs.observe(el); });
  }

  function animateCounter(el) {
    var text = el.textContent.trim();
    var hasPlusSuffix = text.endsWith("+");
    var hasXSuffix = text.endsWith("x");
    var raw = text.replace(/[+,x]/g, "");
    var target = parseInt(raw, 10);
    if (isNaN(target) || target === 0) return;
    var suffix = hasPlusSuffix ? "+" : hasXSuffix ? "x" : "";
    var useCommas = text.includes(",");
    var duration = 1000;
    var start = null;
    function fmt(n) { return useCommas ? n.toLocaleString("en-US") : String(n); }
    function step(ts) {
      if (!start) start = ts;
      var p = Math.min((ts - start) / duration, 1);
      var eased = 1 - Math.pow(1 - p, 3);
      el.textContent = fmt(Math.floor(eased * target)) + suffix;
      if (p < 1) requestAnimationFrame(step);
      else el.textContent = fmt(target) + suffix;
    }
    el.textContent = fmt(0) + suffix;
    requestAnimationFrame(step);
  }

  // ── 7. Typing Effect ──────────────────────────────────────────────────────
  function initTypingEffect() {
    var el = document.querySelector(".hero-subtitle");
    var isHome = ["/" , "/index.html"].some(function (p) {
      return window.location.pathname === p || window.location.pathname.endsWith(p);
    });
    if (!el || !isHome) return;
    if (el.classList.contains("typing-initialized")) return;
    el.classList.add("typing-initialized");

    var phrases = [
      "Rust-powered performance",
      "Python simplicity",
      "Enterprise-grade security",
      "170,000+ requests/sec",
    ];
    var pi = 0, ci = 0, deleting = false;
    el.innerHTML = '<span class="typing-text"></span><span class="typing-cursor" style="color:#E65100;animation:cello-blink 0.7s step-end infinite;">|</span>';
    var target = el.querySelector(".typing-text");

    if (!document.getElementById("cello-blink-style")) {
      var s = document.createElement("style");
      s.id = "cello-blink-style";
      s.textContent = "@keyframes cello-blink{0%,100%{opacity:1}50%{opacity:0}}";
      document.head.appendChild(s);
    }

    function type() {
      var phrase = phrases[pi];
      if (deleting) {
        ci--;
        target.textContent = phrase.substring(0, ci);
        if (ci === 0) { deleting = false; pi = (pi + 1) % phrases.length; setTimeout(type, 60); return; }
        setTimeout(type, 35);
      } else {
        ci++;
        target.textContent = phrase.substring(0, ci);
        if (ci === phrase.length) { deleting = true; setTimeout(type, 2000); return; }
        setTimeout(type, 60);
      }
    }
    setTimeout(type, 600);
  }

  // ── 8. Scroll Animations ──────────────────────────────────────────────────
  function initScrollAnimations() {
    if (!("IntersectionObserver" in window)) return;
    if (!document.getElementById("cello-scroll-style")) {
      var s = document.createElement("style");
      s.id = "cello-scroll-style";
      s.textContent = ".ca{opacity:0;transform:translateY(18px);transition:opacity 0.4s ease,transform 0.4s ease}.ca.v{opacity:1;transform:translateY(0)}";
      document.head.appendChild(s);
    }
    var selectors = [
      ".md-typeset .admonition", ".md-typeset details",
      ".md-typeset .tabbed-set", ".md-typeset .highlight",
      ".md-typeset table", ".md-typeset blockquote", ".md-typeset h2",
    ];
    var els = document.querySelectorAll(selectors.join(","));
    els.forEach(function (el, i) {
      if (el.classList.contains("ca")) return;
      el.classList.add("ca");
      el.style.transitionDelay = (i % 4) * 0.07 + "s";
    });

    var unobservedEls = [];
    els.forEach(function (el) {
      if (!el.classList.contains("v")) {
        unobservedEls.push(el);
      }
    });

    if (unobservedEls.length === 0) return;

    var obs = new IntersectionObserver(function (entries) {
      entries.forEach(function (e) {
        if (e.isIntersecting) { e.target.classList.add("v"); obs.unobserve(e.target); }
      });
    }, { threshold: 0.08, rootMargin: "0px 0px -30px 0px" });
    unobservedEls.forEach(function (el) { obs.observe(el); });
  }

  // ── 9. Search Shortcut (Ctrl/Cmd+K) ──────────────────────────────────────
  var searchShortcutRegistered = false;
  function initSearchShortcut() {
    var input = document.querySelector(".md-search__input");
    if (input) {
      var sc = navigator.platform.includes("Mac") ? " (⌘K)" : " (Ctrl+K)";
      var currentPlaceholder = input.getAttribute("placeholder") || "Search";
      if (!currentPlaceholder.includes("Ctrl+K") && !currentPlaceholder.includes("⌘K")) {
        input.setAttribute("placeholder", currentPlaceholder + sc);
      }
    }
    
    if (searchShortcutRegistered) return;
    searchShortcutRegistered = true;

    document.addEventListener("keydown", function (e) {
      if ((e.metaKey || e.ctrlKey) && (e.key === "k" || e.key === "K")) {
        e.preventDefault();
        var inp = document.querySelector(".md-search__input");
        if (inp) {
          var toggle = document.getElementById("__search");
          if (toggle && !toggle.checked) {
            toggle.checked = true;
            toggle.dispatchEvent(new Event("change"));
          }
          inp.focus();
          inp.select();
        }
      }
    });
  }

  // ── 10. Feedback ──────────────────────────────────────────────────────────
  function initFeedback() {
    var fc = document.querySelector("[data-md-feedback]");
    if (!fc) return;
    if (fc.classList.contains("feedback-initialized")) return;
    fc.classList.add("feedback-initialized");
    fc.querySelectorAll("[data-md-feedback-value]").forEach(function (btn) {
      btn.addEventListener("click", function () {
        var val = this.getAttribute("data-md-feedback-value");
        if (typeof gtag === "function") {
          gtag("event", "feedback", { event_category: "docs", event_label: location.pathname, value: val === "1" ? 1 : 0 });
        }
        fc.innerHTML = '<p style="margin:0;padding:8px 0;color:#2E7D32;font-weight:600;">Thanks for your feedback!</p>';
      });
    });
  }

  // ── Bootstrap ─────────────────────────────────────────────────────────────
  if (typeof document$ !== "undefined") {
    document$.subscribe(function () {
      initTableSort();
      initCopyFeedback();
      initSmoothScroll();
      initExternalLinks();
      initDoubleSidebar();
      initAnimatedCounters();
      initTypingEffect();
      initScrollAnimations();
      initSearchShortcut();
      initFeedback();
    });
  } else {
    document.addEventListener("DOMContentLoaded", function () {
      initTableSort();
      initCopyFeedback();
      initSmoothScroll();
      initExternalLinks();
      initDoubleSidebar();
      initAnimatedCounters();
      initTypingEffect();
      initScrollAnimations();
      initSearchShortcut();
      initFeedback();
    });
  }
})();
