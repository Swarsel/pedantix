;;; pedantix.el --- Format Nix buffers with pedantix  -*- lexical-binding: t; -*-

;; Author: Leon Schwarzäugl
;; Version: 1.2.0
;; Package-Requires: ((emacs "27.1"))
;; Keywords: languages, tools, nix
;; URL: https://github.com/swarsel/pedantix

;;; Commentary:

;; Formats Nix buffers with the `pedantix' command line tool: a base
;; formatter (nixfmt, alejandra, nixpkgs-fmt, ...) plus deterministic
;; ordering of function arguments and attribute-set bindings.
;;
;; Usage:
;;   (require 'pedantix)
;;   M-x pedantix-format-buffer
;;   M-x pedantix-format-region
;;
;; or format on save in nix-mode / nix-ts-mode buffers:
;;   (add-hook 'nix-mode-hook #'pedantix-format-on-save-mode)
;;   (add-hook 'nix-ts-mode-hook #'pedantix-format-on-save-mode)
;;
;; Note that `pedantix-format-region' requires the region to be a
;; self-contained Nix expression (an attribute set, a list, a function,
;; ...), because pedantix and the base formatters parse complete
;; expressions; a bare run of bindings like `b = 1; a = 2;' cannot be
;; formatted on its own.

;;; Code:

(defgroup pedantix nil
  "Format Nix buffers with pedantix."
  :group 'languages
  :prefix "pedantix-")

(defcustom pedantix-program "pedantix"
  "Name or path of the pedantix executable."
  :type 'string)

(defcustom pedantix-arguments nil
  "Extra command line arguments passed to pedantix.
For example (\"--config\" \"/path/to/pedantix.toml\"),
(\"--formatter\" \"alejandra\") or (\"--set\" \"lets.sort=true\")."
  :type '(repeat string))

(defcustom pedantix-show-errors t
  "Whether to pop up a buffer with stderr output when formatting fails."
  :type 'boolean)

(defun pedantix--build-args ()
  "Assemble the argument list for the pedantix process."
  (append pedantix-arguments
          (when buffer-file-name
            (list "--stdin-filepath" buffer-file-name))))

(defun pedantix--report-failure (err-file status hint)
  "Show the stderr in ERR-FILE after pedantix exited with STATUS.
HINT is appended to the echo-area message when non-nil."
  (when pedantix-show-errors
    (with-current-buffer (get-buffer-create "*pedantix errors*")
      (let ((inhibit-read-only t))
        (erase-buffer)
        (insert-file-contents err-file)
        (special-mode))
      (display-buffer (current-buffer))))
  (message "pedantix: formatting failed (exit code %s)%s"
           status (if hint (concat "; " hint) "")))

(defun pedantix--call (beg end fn hint)
  "Run pedantix on the region BEG..END and call FN with the output buffer.
FN is only called when pedantix succeeds.  HINT is passed to
`pedantix--report-failure' otherwise."
  (let ((out-buffer (generate-new-buffer " *pedantix*"))
        (err-file (make-temp-file "pedantix-errors")))
    (unwind-protect
        (let ((status (apply #'call-process-region
                             beg end
                             pedantix-program
                             nil (list out-buffer err-file) nil
                             (pedantix--build-args))))
          (if (zerop status)
              (funcall fn out-buffer)
            (pedantix--report-failure err-file status hint)))
      (kill-buffer out-buffer)
      (delete-file err-file))))

;;;###autoload
(defun pedantix-format-buffer ()
  "Format the current buffer with pedantix.
Point is preserved on a best-effort basis via
`replace-region-contents'."
  (interactive)
  (let ((source (current-buffer)))
    (pedantix--call
     (point-min) (point-max)
     (lambda (out-buffer)
       (let ((changed (not (zerop (compare-buffer-substrings
                                   out-buffer nil nil
                                   source nil nil)))))
         (when changed
           (replace-region-contents (point-min) (point-max)
                                    (lambda () out-buffer)))
         (message (if changed "pedantix: formatted" "pedantix: already formatted"))))
     nil)))

;;;###autoload
(defun pedantix-format-region (beg end)
  "Format the region from BEG to END with pedantix.
The region must be a self-contained Nix expression.  The formatted
text is re-indented so that its continuation lines line up with the
column the region starts at."
  (interactive "r")
  (let ((base-column (save-excursion
                       (goto-char beg)
                       (current-column)))
        (had-final-newline (eq (char-before end) ?\n))
        (marker-beg (copy-marker beg))
        (marker-end (copy-marker end)))
    (pedantix--call
     beg end
     (lambda (out-buffer)
       (let ((formatted
              (with-current-buffer out-buffer
                (when (> base-column 0)
                  (goto-char (point-min))
                  (forward-line 1)
                  (let ((indent-tabs-mode nil))
                    (indent-rigidly (point) (point-max) base-column)))
                (goto-char (point-max))
                (when (and (not had-final-newline) (eq (char-before) ?\n))
                  (delete-char -1))
                (buffer-string))))
         (if (string= formatted (buffer-substring-no-properties
                                 marker-beg marker-end))
             (message "pedantix: region already formatted")
           (goto-char marker-beg)
           (delete-region marker-beg marker-end)
           (insert formatted)
           (message "pedantix: region formatted"))))
     "is the region a self-contained Nix expression?")
    (set-marker marker-beg nil)
    (set-marker marker-end nil)))

;;;###autoload
(define-minor-mode pedantix-format-on-save-mode
  "Run `pedantix-format-buffer' before saving the buffer."
  :lighter " Pdx"
  (if pedantix-format-on-save-mode
      (add-hook 'before-save-hook #'pedantix-format-buffer nil t)
    (remove-hook 'before-save-hook #'pedantix-format-buffer t)))

(provide 'pedantix)
;;; pedantix.el ends here
