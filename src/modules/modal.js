const returnFocus = new WeakMap();
const keyHandlers = new WeakMap();

function focusableElements(modal) {
  return [...modal.querySelectorAll('button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), a[href], [tabindex]:not([tabindex="-1"])')]
    .filter(element => !element.hidden && element.getAttribute('aria-hidden') !== 'true');
}

export function openAccessibleModal(modal, initialFocus = null, onEscape = null) {
  if (!modal) return;
  returnFocus.set(modal, document.activeElement);
  modal.hidden = false;
  modal.inert = false;
  modal.setAttribute('aria-hidden', 'false');
  modal.classList.add('active');
  const handler = (event) => {
    if (event.key === 'Escape') {
      event.preventDefault();
      if (onEscape) onEscape();
      else closeAccessibleModal(modal);
      return;
    }
    if (event.key !== 'Tab') return;
    const focusable = focusableElements(modal);
    if (!focusable.length) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  };
  keyHandlers.set(modal, handler);
  modal.addEventListener('keydown', handler);
  queueMicrotask(() => (initialFocus || focusableElements(modal)[0] || modal).focus());
}

export function closeAccessibleModal(modal) {
  if (!modal) return;
  const handler = keyHandlers.get(modal);
  if (handler) modal.removeEventListener('keydown', handler);
  keyHandlers.delete(modal);
  modal.classList.remove('active');
  modal.setAttribute('aria-hidden', 'true');
  modal.inert = true;
  modal.hidden = true;
  const previous = returnFocus.get(modal);
  returnFocus.delete(modal);
  if (previous?.isConnected) previous.focus();
}
