// ============================================
// cmod Website — JavaScript
// ============================================

document.addEventListener('DOMContentLoaded', () => {
  initMobileNav();
  initCopyButtons();
  initSmoothScroll();
  initScrollSpy();
});

// --- Mobile Navigation ---
function initMobileNav() {
  const hamburger = document.querySelector('.nav__hamburger');
  const navLinks = document.querySelector('.nav__links');
  if (!hamburger || !navLinks) return;

  hamburger.addEventListener('click', () => {
    navLinks.classList.toggle('active');
    const isOpen = navLinks.classList.contains('active');
    hamburger.textContent = isOpen ? '\u2715' : '\u2630';
    hamburger.setAttribute('aria-expanded', isOpen);
  });

  // Close on link click
  navLinks.querySelectorAll('a').forEach(link => {
    link.addEventListener('click', () => {
      navLinks.classList.remove('active');
      hamburger.textContent = '\u2630';
    });
  });
}

// --- Copy Code Buttons ---
function initCopyButtons() {
  document.querySelectorAll('.code-block__copy').forEach(btn => {
    btn.addEventListener('click', () => {
      const codeBlock = btn.closest('.code-block');
      const code = codeBlock.querySelector('code');
      if (!code) return;

      navigator.clipboard.writeText(code.textContent).then(() => {
        const original = btn.textContent;
        btn.textContent = 'Copied!';
        setTimeout(() => { btn.textContent = original; }, 2000);
      });
    });
  });

  // Install command copy
  document.querySelectorAll('.install-cmd__copy').forEach(btn => {
    btn.addEventListener('click', () => {
      const cmd = btn.closest('.install-cmd');
      const text = cmd.querySelector('.install-cmd__text');
      if (!text) return;

      navigator.clipboard.writeText(text.textContent).then(() => {
        btn.textContent = '\u2713';
        setTimeout(() => { btn.textContent = '\u2398'; }, 2000);
      });
    });
  });
}

// --- Smooth Scroll for anchor links ---
function initSmoothScroll() {
  document.querySelectorAll('a[href^="#"]').forEach(anchor => {
    anchor.addEventListener('click', (e) => {
      const target = document.querySelector(anchor.getAttribute('href'));
      if (target) {
        e.preventDefault();
        target.scrollIntoView({ behavior: 'smooth', block: 'start' });
      }
    });
  });
}

// --- Scroll Spy for nav ---
function initScrollSpy() {
  const sections = document.querySelectorAll('section[id]');
  const navLinks = document.querySelectorAll('.nav__links a[href^="#"]');
  if (!sections.length || !navLinks.length) return;

  const observer = new IntersectionObserver((entries) => {
    entries.forEach(entry => {
      if (entry.isIntersecting) {
        const id = entry.target.id;
        navLinks.forEach(link => {
          link.style.color = link.getAttribute('href') === `#${id}`
            ? 'var(--color-text)' : '';
        });
      }
    });
  }, { rootMargin: '-30% 0px -70% 0px' });

  sections.forEach(section => observer.observe(section));
}
