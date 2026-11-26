# Nhật ký thay đổi — GumballZ Get

Nguồn chính xác cho mục "Nhật ký thay đổi" trong app: mỗi bản phát hành lấy
mục tương ứng dưới đây làm nội dung. Viết ngắn gọn, tiếng Việt, mỗi thay đổi
một dòng. Khi làm tính năng mới hoặc sửa lỗi, thêm dòng vào mục của phiên
bản hiện tại trong cùng commit đó.

## [1.0.1] - 2026-11-26

- Định dạng tải luôn hiện ngay trên tên file để chọn ngay, cho mọi nền tảng (YouTube, Scribd...); mục “Nâng cao” bên dưới vẫn giữ các tuỳ chọn khác.
- Link tài liệu (Scribd, Google Docs) tự chọn sẵn PDF khi vừa nhận diện link, chỉ hiện PDF/DOCX, không còn hiện lựa chọn video/âm thanh/chất lượng video.
- Sửa các chỗ còn ghi v0.4.5: mô tả kho GitHub trong Cài đặt và phiên bản tiện ích Firefox đi kèm (giờ lấy theo đúng phiên bản app, tự cập nhật mỗi lần tăng phiên bản).

## [1.0.0] - 2026-11-26

- Tải link Facebook khi Cốc Cốc đang mở: app tự copy ngầm profile cookie sang thư mục tạm rồi mới đọc, không còn lỗi Permission denied.
- Tải tài liệu Scribd thành PDF, chữ tiếng Việt hiển thị đúng (font Be Vietnam Pro nhúng sẵn).
- Nút tải Windows trên web trỏ về bản cài đặt (.exe setup), không còn trỏ nhầm bản portable.
- Sửa link tải dự phòng trên web (tên file đúng dạng `GumballZ.Get_...`).
- Danh sách phiên bản trên web lưu lại bản đã tải được, khi GitHub lỗi thì hiện bản lưu thay vì báo lỗi đỏ.
- Danh sách phiên bản trên web chỉ hiện bản mới nhất, các bản cũ thu gọn sau nút “Xem các phiên bản cũ”; trang nhật ký thay đổi cũng vậy (bản mới nhất mở sẵn, các bản cũ thu gọn).
- Gộp mọi lần gọi API GitHub về 1 chỗ duy nhất có cache (bộ nhớ 5 phút + lưu 24 giờ): mở web/app nhiều lần trong 1 giờ không còn chạm giới hạn 60 lần/giờ.
- Tải YouTube tách 3 chế độ rõ ràng: clip hiện tại, playlist, mix.
- Mix tự động (RD...) không còn giới hạn 50 bài: liệt kê đủ hơn 1000 bài, tải hết với tiến độ từng bài.
- Sửa lỗi: chọn tải playlist/mix từ link watch?v=...&list=... trước đây chỉ tải 1 video, giờ tải đúng các mục đã chọn.
- Mục chọn định dạng theo loại link: link tài liệu chỉ hiện định dạng tài liệu (Scribd: PDF; Google Docs: PDF / DOCX), không còn hiện lựa chọn video/âm thanh; link media giữ nguyên như cũ.
- Ảnh chụp màn hình Flatpak là giao diện app desktop hiện tại (thay cho ảnh giao diện LibreGet cũ).
