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

## [1.0.0] - 2026-11-26

- V1.0.0
