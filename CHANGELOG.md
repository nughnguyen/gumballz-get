# Nhật ký thay đổi — GumballZ Get

Nguồn chính xác cho mục "Nhật ký thay đổi" trong app: mỗi bản phát hành lấy
mục tương ứng dưới đây làm nội dung. Viết ngắn gọn, tiếng Việt, mỗi thay đổi
một dòng. Khi làm tính năng mới hoặc sửa lỗi, thêm dòng vào mục của phiên
bản hiện tại trong cùng commit đó.

## [1.0.1] - 2026-11-26

- Định dạng tải luôn hiện ngay trên tên file để chọn ngay, cho mọi nền tảng (YouTube, Scribd...); mục “Nâng cao” bên dưới vẫn giữ các tuỳ chọn khác.
- Link tài liệu (Scribd, Google Docs) tự chọn sẵn PDF khi vừa nhận diện link, chỉ hiện PDF/DOCX, không còn hiện lựa chọn video/âm thanh/chất lượng video.
- Sửa các chỗ còn ghi v0.4.5: mô tả kho GitHub trong Cài đặt và phiên bản tiện ích Firefox đi kèm (giờ lấy theo đúng phiên bản app, tự cập nhật mỗi lần tăng phiên bản).
- Sửa lỗi lịch sử tải Facebook/Instagram biến mất khi tải thêm tài liệu khác: đồng bộ hàng đợi giờ chỉ dọn các mục thuộc hàng đợi, không xoá các lượt tải mạng xã hội.
- Nhấn vào tên một file đã tải xong (trang Tải xuống hoặc Lịch sử) sẽ mở thẳng file bằng ứng dụng mặc định, không cần mở thư mục rồi nhấn vào file nữa.
- Tab ngang Tải xuống / Lịch sử / Công cụ giờ có viên thuốc trượt theo kiểu giọt nước: trượt mượt qua các tab với hiệu ứng nảy nhẹ khi đổi tab.
- Dọn log khi chạy dev: không còn cảnh báo gắn metadata cho file PDF (ffmpeg chỉ gắn được cho audio/video), không còn cảnh báo kho plugin chưa có (repo chưa xuất bản), không còn 2 dòng ERROR registry khi xoá khoá cũ, hết cảnh báo `mut` thừa ở trình ghi PDF Scribd và các đoạn CSS chết Svelte báo thừa.
- Sửa bảng trong PDF Scribd bị vỡ: Scribd không có lưới bảng thật — bảng là chữ xếp cột, trình ghi cũ áp letter-spacing thành Tc nên chữ tản ra ("J V 0 0 1 A b e t") và nuốt khoảng trắng giữa các cột ("EpisodeWord"). Giờ chữ trong ô bảng được vẽ từng cụm từ đúng vị trí cột, khoảng cách theo đúng spacer của Scribd.
- Link Scribd giờ tải được cả DOCX (chọn trong mục định dạng, bên cạnh PDF): chữ, bảng (thành bảng Word thật) và ảnh trang đều có trong file .docx, mở được bằng Word.
- Tab “Bạn bè” trong Mạng xã hội: xem danh sách bạn bè (Facebook) / following và followers (Instagram) kèm ảnh đại diện và bài đăng của từng người, chọn từng bài tải hoặc tải hết luôn; cookie đọc từ kho chung (tiện ích tự bắt hoặc đăng nhập trong app).
- Nút “Cài tiện ích” trong Cài đặt: app tự chuẩn bị sẵn thư mục tiện ích (bản portable tự tải file từ bản release trên GitHub) rồi mở sẵn trang chrome://extensions, chỉ cần bật Developer Mode rồi bấm “Load unpacked” chọn thư mục là xong; tiện ích đã ghép nối thì hiện “Đã cài đặt”.
- Sidebar: mục Plugin chuyển xuống giữa (sau các tab trang chủ, trước phần Ứng dụng), phần Ứng dụng và Cài đặt xuống dưới cuối; mục đang chọn có viên thuốc trượt theo với hiệu ứng nảy khi chuyển tab.
- Trình tạo keys.ts giờ chỉ ghi file khi nội dung thật sự đổi (so sánh đã chuẩn hoá xuống dòng): Vite chạy trình tạo này mỗi lần mở app, trước đây nó ghi lại file dù không đổi gì nên keys.ts cứ hiện là file đã sửa trên máy Windows.
- Sửa lỗi popup Cài đặt hiệu năng Telegram (và mọi popup khác) trôi khỏi màn hình khi cuộn trang: khung cuộn chính có `transform: translateZ(0)` khiến popup `position: fixed` bị neo theo khung cuộn thay vì màn hình — đã bỏ transform ở cả hai chỗ, popup giờ dính chặt màn hình khi cuộn.
- Dán link Scribd giờ hiện ảnh thumbnail là trang đầu của tài liệu trong thẻ xem trước, thay cho ảnh mặc định; tài liệu chỉ có chữ thì vẫn hiện ảnh mặc định như trước.

## [1.0.0] - 2026-11-26

- V1.0.0
