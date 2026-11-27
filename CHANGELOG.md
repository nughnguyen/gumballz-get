# Nhật ký thay đổi — GumballZ Get

Nguồn chính xác cho mục "Nhật ký thay đổi" trong app: mỗi bản phát hành lấy
mục tương ứng dưới đây làm nội dung. Viết ngắn gọn, tiếng Việt, mỗi thay đổi
một dòng. Khi làm tính năng mới hoặc sửa lỗi, thêm dòng vào mục của phiên
bản hiện tại trong cùng commit đó.

## [Unreleased]

- Đăng nhập Gumroad bằng email/mật khẩu giờ mở cửa sổ trình duyệt trong app để đăng nhập trên trang Gumroad rồi đổi mã lấy token (Gumroad đã bỏ kiểu đăng nhập gửi thẳng mật khẩu nên trước đây báo lỗi 400 unsupported_grant_type); đăng nhập bằng token vẫn giữ nguyên.
- Sửa đăng nhập Gumroad: địa chỉ redirect đúng là `http://127.0.0.1` (địa chỉ đã đăng ký cho app này), không phải `gumroadmobile://` nên trước đây Gumroad báo lỗi redirect không khớp.
- Trang Công cụ không còn công cụ mặc định — chỉ hiện plugin đã cài (trống thì báo chưa có plugin nào); mục Plugins ở sidebar đã bỏ, mọi việc về plugin nằm trong trang Công cụ; 5 công cụ có sẵn (Convert, Courses, Telegram, Listening Studio, Asset Forge) giờ là plugin phải tải về mới có (hiện tại cài từ file .zip).
- Đăng nhập Gumroad trong app giờ tự điền sẵn email + mật khẩu đã gõ vào trang đăng nhập của Gumroad (chỉ điền, anh vẫn tự bấm nút đăng nhập trên trang đó); mật khẩu chỉ nằm trong bộ nhớ lúc đăng nhập, không gửi hay lưu ở đâu.
- Thêm nguồn tải mới: Pinterest (sửa bộ đọc trang — giờ lấy ảnh gốc thay vì bản thu nhỏ, video chọn đúng chất lượng), Spotify (thông tin bài hát + ảnh bìa + bản nghe thử 30 giây — nhạc đầy đủ không tải được vì Spotify dùng DRM), Studocu (tài liệu PDF/DOCX từ bản xem trước, trang bị khóa chỉ có bản mờ), SoundCloud (nhạc .m4a). Dailymotion chưa thêm được vì yt-dlp đang lỗi với trang này.

## [1.0.2] - 2026-11-26

- Logo mới: mèo xanh GumballZ (chữ G là mặt mèo, mũi tên tải màu cam) thay cho logo chữ F cũ, đổi ở toàn bộ app, web và tiện ích.
- Sửa logo ở hero trang web bị lớp nền tím che mất: CSS cũ chỉ tạo vị trí cho bản SVG của logo, bản ảnh PNG mới không được tạo vị trí nên bị che khuất.
- Phần Tải xuống: mục tải xong xếp theo thời gian hoàn thành, mới nhất lên trên cùng (trước đây xếp theo thời gian cập nhật chung của cả hàng đợi nên mục mới xong lại nằm dưới cùng).
- Link TikTok ảnh giờ tải được: hiện danh sách ảnh (tải từng ảnh hoặc tải hết), video slideshow (ảnh tự lướt kèm nhạc) và âm thanh riêng; link video TikTok có tuỳ chọn có/không watermark.
- Link video TikTok giờ hiện hai nút rõ ràng "Video (có watermark)" / "Video (không watermark)" ngay trong ô tải, thay vì phải tìm trong bảng định dạng (chỉ hiện nút nào TikTok thật sự có).
- Sửa link TikTok ảnh trên máy thật: bản sửa trước nằm nhầm ở module chết không được biên dịch — đã chuyển sang module thật trong gumballz-core (thêm tiêu đề trình duyệt để TikTok không chặn, đường yt-dlp dự phòng cho âm thanh), lỗi bị TikTok chặn giờ hiện rõ thay vì báo "URL không được hỗ trợ"; xoá luôn module chết đó.
- Thiết kế lại giao diện phần Khóa học: thẻ đăng nhập nền tảng (tiêu đề + tab credentials/Token kiểu mới), danh sách khóa học (tiêu đề Bento, ô tìm kiếm, huy hiệu trạng thái), thẻ khóa học và hộp thoại sections; mỗi khóa học có thêm nút "Mở trên nền tảng" để mở trang chủ khóa học đó; nút back ở trang Khóa học và trang Công cụ làm lại kiểu pill mới.
- Nút "Quay lại Khóa học" giờ ôm sát nội dung thay vì kéo dài hết chiều rộng trang.
- Bấm vào ảnh bìa hoặc tên khóa học sẽ mở trang web của khóa học đó trên trình duyệt (nút tải và nút chọn sections vẫn như cũ).
- Thẻ đăng nhập Khóa học có thêm nút "Mở trang web {tên nền tảng}" để mở trang chủ nền tảng trên trình duyệt (tạo tài khoản khi chưa có).
- Sửa đăng nhập Khóa học bị treo ở "Đang xác thực..." khi tắt cửa sổ đăng nhập (Hotmart/Udemy): giờ báo lỗi và cho nhập lại ngay; thêm nút "Hủy" khi đang đăng nhập.
- Phiên đăng nhập Khóa học được giữ lại sau khi khởi động lại app; lỗi mạng tạm thời không còn xóa phiên đã lưu (chỉ xóa khi hết hạn 401/403).
- Mở link khóa học trên trình duyệt giờ hoạt động (bổ sung quyền mở link cho các trang nền tảng khóa học — trước đây nút "Mở trên nền tảng" và link ảnh bìa/tên khóa học có thể đã bị chặn).

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
- Hiện tốc độ tải thật và số byte đã tải: Scribd, Google Docs/Slides, Instagram, TikTok, X, Threads, Bluesky, Reddit, torrent, P2P, Telegram và gallery-dl giờ báo số byte thật tải về để app tính tốc độ; lúc biên dịch file thì tốc độ về 0 là đúng vì không tải mạng nữa.
- Thiết kế lại mục Giới thiệu trong app: bố cục lưới Bento mới (hero, 3 ô tính năng, 3 ô dẫn tới Nhật ký thay đổi / Dự án / Điều khoản, credits và donate xếp cạnh nhau), giữ nguyên mọi link và thông tin phiên bản.

## [1.0.0] - 2026-11-26

- V1.0.0
